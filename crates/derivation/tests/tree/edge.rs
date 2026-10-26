use super::{atom, derive, family, kind, run};
use locus_derivation::tree::{Condition, Tree};
use serde_json::{Value, json};

#[test]
fn unresolved() {
    let atoms = vec![
        atom(
            1,
            json!({"locus.trace": "lost", "locus.span": "a", "locus.parent": "gone"}),
            json!({"mark": true}),
        ),
        atom(
            2,
            json!({"locus.trace": "torn", "locus.span": "b", "locus.parent": "x"}),
            json!({"mark": true}),
        ),
        atom(
            3,
            json!({"locus.trace": "torn", "locus.span": "c", "locus.parent": "y"}),
            json!(null),
        ),
        atom(
            4,
            json!({"locus.trace": "one", "locus.span": "shared"}),
            json!(null),
        ),
        atom(
            5,
            json!({"locus.trace": "two", "locus.span": "shared"}),
            json!(null),
        ),
        atom(
            6,
            json!({"locus.trace": "split", "locus.span": "d", "locus.parent": "shared"}),
            json!({"mark": true}),
        ),
        atom(
            7,
            json!({"locus.trace": "self", "locus.span": "e", "locus.parent": "e"}),
            json!({"mark": true}),
        ),
    ];
    let records = derive(&atoms, &["payload.mark=true"], &[]);
    let trees = kind(&records, "tree");
    let roots: Vec<&Value> = trees.iter().map(|tree| &tree["root"]).collect();
    assert_eq!(
        roots,
        [
            &json!("lost"),
            &json!("torn"),
            &json!("split"),
            &json!("self")
        ]
    );
    assert!(
        trees
            .iter()
            .all(|tree| tree["detached"] == true && tree["traces"] == 1)
    );
    let links = &records.last().expect("summary")["links"];
    assert_eq!(
        *links,
        json!({"resolved": 0, "missing": 2, "ambiguous": 1, "conflicting": 1})
    );
}

#[test]
fn cyclic() {
    let atoms = vec![
        atom(
            1,
            json!({"locus.trace": "b", "locus.span": "sb", "locus.parent": "sa"}),
            json!({"mark": 1}),
        ),
        atom(
            2,
            json!({"locus.trace": "a", "locus.span": "sa", "locus.parent": "sb"}),
            json!(null),
        ),
        atom(
            3,
            json!({"locus.trace": "c", "locus.span": "sc", "locus.parent": "sb"}),
            json!({"mark": 1}),
        ),
    ];
    let records = derive(&atoms, &["payload.mark=1"], &[]);
    let trees = kind(&records, "tree");
    assert_eq!(trees.len(), 1);
    assert_eq!(
        (trees[0]["root"].clone(), trees[0]["traces"].clone()),
        (json!("a"), json!(3))
    );
    assert_eq!(trees[0]["detached"], false);
}

#[test]
fn grammar() {
    let atoms = vec![atom(
        1,
        json!({"locus.trace": "t", "app.role": "k"}),
        json!({"code": 0, "ok": false, "text": "0", "list": [0]}),
    )];
    let holds = |seeds: &[&str]| kind(&derive(&atoms, seeds, &[]), "tree").len();
    assert_eq!(holds(&["payload.code=0"]), 1);
    assert_eq!(holds(&["payload.text=0"]), 1);
    assert_eq!(holds(&["payload.ok=false"]), 1);
    assert_eq!(holds(&["context.app.role=k", "payload.code=0"]), 1);
    assert_eq!(holds(&["payload.list=[0]"]), 0);
    assert_eq!(holds(&["payload.code=0", "payload.ok=true"]), 0);
    assert_eq!(holds(&["context.app.role=other"]), 0);
    for invalid in [
        "payload.=1",
        "payload",
        "trace.x=1",
        "context.=k",
        "context.app.role=",
    ] {
        assert!(Condition::parse(invalid).is_err(), "{invalid}");
    }
    assert!(Tree::new(Vec::new(), Vec::new()).is_err());
    let empty = derive(&[], &["payload.code=0"], &[]);
    assert_eq!(empty.len(), 1);
    assert_eq!(empty[0]["trees"], 0);
}

#[test]
fn limits() {
    let seeds = ["payload.outcome=refused"];
    let show = ["payload.event=cli.finish"];
    assert!(run(&family(), &seeds, &show, |tree| tree.trees(1)).is_ok());
    let mut atoms = family();
    atoms.push(atom(
        9,
        json!({"locus.trace": "far"}),
        json!({"outcome": "refused"}),
    ));
    let refused = run(&atoms, &seeds, &show, |tree| tree.trees(1));
    assert!(refused.expect_err("trees").contains("more than 1 trees"));
    let held = run(&family(), &seeds, &show, |tree| tree.atoms(2));
    assert!(
        held.expect_err("atoms")
            .contains("more than 2 seed or shown")
    );
    let keys = run(&family(), &seeds, &show, |tree| tree.keys(8));
    assert!(
        keys.expect_err("keys")
            .contains("more than 8 trace and span keys")
    );
    let exact = run(&family(), &seeds, &show, |tree| {
        tree.trees(1).atoms(3).keys(9)
    });
    assert!(exact.is_ok());
}

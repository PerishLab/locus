mod edge;

use locus::Atom;
use locus_derivation::tree::{Condition, Tree};
use serde_json::{Value, json};

pub fn atom(at: u64, context: Value, payload: Value) -> Atom {
    serde_json::from_value(json!({
        "at": at,
        "context": context,
        "choices": [],
        "payload": payload,
    }))
    .expect("atom")
}

fn conditions(texts: &[&str]) -> Vec<Condition> {
    texts
        .iter()
        .map(|text| Condition::parse(text).expect("condition"))
        .collect()
}

pub fn run(
    atoms: &[Atom],
    seeds: &[&str],
    show: &[&str],
    bound: impl FnOnce(Tree) -> Tree,
) -> Result<Vec<Value>, String> {
    let mut tree = bound(Tree::new(conditions(seeds), conditions(show))?);
    for atom in atoms {
        tree.observe(atom.clone())?;
    }
    let report = tree.finish()?.retained(Some(7));
    Ok(report
        .records()
        .iter()
        .map(|record| serde_json::to_value(record).expect("record"))
        .collect())
}

pub fn derive(atoms: &[Atom], seeds: &[&str], show: &[&str]) -> Vec<Value> {
    run(atoms, seeds, show, |tree| tree).expect("derive")
}

pub fn kind<'a>(records: &'a [Value], kind: &str) -> Vec<&'a Value> {
    records
        .iter()
        .filter(|record| record["kind"] == kind)
        .collect()
}

pub fn family() -> Vec<Atom> {
    vec![
        atom(
            1,
            json!({"locus.trace": "root", "locus.span": "r1"}),
            json!({"event": "cli.start"}),
        ),
        atom(
            2,
            json!({"locus.trace": "child", "locus.span": "c1", "locus.parent": "r1"}),
            json!(null),
        ),
        atom(
            3,
            json!({"locus.trace": "leaf", "locus.span": "l1", "locus.parent": "c1"}),
            json!({"outcome": "refused"}),
        ),
        atom(
            4,
            json!({"locus.trace": "other", "locus.span": "o1"}),
            json!({"event": "cli.finish", "code": 1}),
        ),
        atom(
            5,
            json!({"locus.trace": "leaf", "locus.span": "l2"}),
            json!(null),
        ),
        atom(
            6,
            json!({"locus.trace": "root", "locus.span": "r1"}),
            json!({"event": "cli.finish", "code": 0}),
        ),
    ]
}

#[test]
fn nested() {
    let records = derive(
        &family(),
        &["payload.outcome=refused"],
        &["payload.event=cli.finish"],
    );
    let trees = kind(&records, "tree");
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0]["root"], "root");
    assert_eq!(trees[0]["traces"], 3);
    assert_eq!(trees[0]["records"], 5);
    assert_eq!(
        (trees[0]["from"].clone(), trees[0]["until"].clone()),
        (json!(1), json!(6))
    );
    assert_eq!(trees[0]["detached"], false);
    assert_eq!(trees[0]["links"]["resolved"], 2);
    let atoms = kind(&records, "atom");
    assert_eq!(atoms.len(), 2);
    assert_eq!(
        (atoms[0]["seed"].clone(), atoms[0]["atom"]["at"].clone()),
        (json!(true), json!(3))
    );
    assert_eq!(
        (
            atoms[1]["shown"].clone(),
            atoms[1]["atom"]["payload"]["code"].clone()
        ),
        (json!(true), json!(0))
    );
    let summary = records.last().expect("summary");
    assert_eq!(summary["kind"], "summary");
    assert_eq!(
        (summary["records"].clone(), summary["seeds"].clone()),
        (json!(6), json!(1))
    );
    assert_eq!(
        (summary["trees"].clone(), summary["shown"].clone()),
        (json!(1), json!(1))
    );
    assert_eq!(summary["retained"], 7);
}

#[test]
fn multiplicity() {
    let mut atoms = family();
    atoms.push(atoms[2].clone());
    atoms.push(atom(
        9,
        json!({"locus.trace": "other"}),
        json!({"outcome": "refused"}),
    ));
    atoms.push(atom(
        9,
        json!({"locus.span": "free"}),
        json!({"outcome": "refused"}),
    ));
    let records = derive(&atoms, &["payload.outcome=refused"], &[]);
    let trees = kind(&records, "tree");
    assert_eq!(trees.len(), 2);
    assert_eq!(
        (trees[0]["root"].clone(), trees[0]["seeds"].clone()),
        (json!("root"), json!(2))
    );
    assert_eq!(
        (trees[1]["root"].clone(), trees[1]["records"].clone()),
        (json!("other"), json!(2))
    );
    let summary = records.last().expect("summary");
    assert_eq!(
        (summary["seeds"].clone(), summary["untraced"].clone()),
        (json!(3), json!(1))
    );
}

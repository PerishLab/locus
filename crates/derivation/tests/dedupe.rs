use locus::Atom;
use locus_derivation::evidence::Evidence;
use locus_derivation::inspect::{Config, Inspection};
use locus_derivation::span::Span;
use serde_json::{Value, json};

fn edge(id: &str, at: u64, span: &str, kind: &str) -> Atom {
    serde_json::from_value(json!({
        "at": at,
        "id": id,
        "producer": "web",
        "context": {"locus.trace": "t1", "locus.span": span},
        "choices": [],
        "source": {
            "file": "app.ts",
            "line": 1,
            "column": 1,
            "module": "web",
            "function": "auth",
            "edge": kind
        },
        "payload": {"name": "auth"}
    }))
    .unwrap()
}

fn rendered(span: Span) -> Vec<Value> {
    span.finish()
        .records()
        .into_iter()
        .map(|record| serde_json::to_value(record).unwrap())
        .collect()
}

fn config() -> Config {
    Config::parse(
        "test",
        r#"
version = 1
disable = ["span.size", "span.prefix", "span.held"]
[[analyzer]]
id = "trace.size"
mapping = { kind = "representation-bytes", group = "locus.trace" }
threshold = { above = 0 }
[[analyzer]]
id = "trace.prefix"
mapping = { kind = "dominant-content-prefix-percent", group = "locus.trace" }
threshold = { above = 0 }
[[analyzer]]
id = "trace.held"
mapping = { kind = "held-time-nanoseconds", group = "locus.trace" }
threshold = { above = 0 }
"#,
    )
    .unwrap()
}

#[test]
fn spans() {
    let enter = edge("a1", 10, "s1", "enter");
    let ret = edge("a2", 40, "s1", "return");
    for sequence in [
        vec![enter.clone(), enter.clone(), ret.clone()],
        vec![enter.clone(), ret.clone(), enter.clone(), ret.clone()],
        vec![enter.clone(), ret.clone(), ret.clone()],
    ] {
        let mut span = Span::default();
        for atom in &sequence {
            span.observe(atom).unwrap();
        }
        let rows = rendered(span);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["kind"], "declaration");
        assert_eq!(rows[0]["spans"], 1);
        assert_eq!(rows[0]["inclusive"], 30);
        assert_eq!(rows[0]["held"], 30);
        assert_eq!(rows[1]["kind"], "summary");
        assert_eq!(rows[1]["matched"], 2);
        assert_eq!(rows[1]["headless"], 0);
        assert_eq!(rows[1]["unclosed"], 0);
        assert_eq!(rows[1]["tangled"], 0);
    }
}

#[test]
fn inspect() {
    let config = config();
    let atoms = [
        edge("b1", 10, "s1", "enter"),
        edge("b2", 20, "s1", "return"),
        edge("b3", 30, "s2", "enter"),
        edge("b4", 50, "s2", "return"),
        edge("b5", 60, "s3", "enter"),
        edge("b6", 70, "s3", "return"),
    ];
    let mut once = Inspection::new(&config);
    for atom in &atoms {
        once.observe(atom, 100).unwrap();
    }
    let clean = once.finish();

    let mut replayed = Inspection::new(&config);
    for atom in &atoms {
        replayed.observe(atom, 100).unwrap();
        replayed.observe(atom, 100).unwrap();
    }
    let dup = replayed.finish();

    let left: Vec<Value> = clean
        .findings()
        .iter()
        .map(|f| serde_json::to_value(f).unwrap())
        .collect();
    let right: Vec<Value> = dup
        .findings()
        .iter()
        .map(|f| serde_json::to_value(f).unwrap())
        .collect();
    assert_eq!(left, right);

    let mut evidence = Evidence::new(dup.findings()).unwrap();
    for atom in &atoms {
        evidence.observe(atom, 100).unwrap();
        evidence.observe(atom, 100).unwrap();
    }
    let groups: Vec<Value> = evidence
        .finish()
        .map(|g| serde_json::to_value(g).unwrap())
        .collect();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["records"], 6);
    assert_eq!(groups[0]["bytes"], 600);
    assert_eq!(groups[0]["contributions"][0]["enters"], 3);
    assert_eq!(groups[0]["contributions"][1]["returns"], 3);
}

#[test]
fn conflict() {
    let first = edge("c1", 10, "s1", "enter");
    let second = edge("c1", 15, "s1", "enter");
    let mut span = Span::default();
    span.observe(&first).unwrap();
    assert!(span.observe(&second).is_err());

    let config = config();
    let mut inspection = Inspection::new(&config);
    inspection.observe(&first, 100).unwrap();
    assert!(inspection.observe(&second, 100).is_err());

    let mut base = Inspection::new(&config);
    base.observe(&first, 100).unwrap();
    let mut evidence = Evidence::new(base.finish().findings()).unwrap();
    evidence.observe(&first, 100).unwrap();
    assert!(evidence.observe(&second, 100).is_err());
}

#[test]
fn producers() {
    let mut first = serde_json::to_value(edge("p1", 10, "s1", "enter")).unwrap();
    let mut second = serde_json::to_value(edge("p1", 20, "s2", "enter")).unwrap();
    first["producer"] = json!("alpha");
    second["producer"] = json!("beta");
    let mut span = Span::default();
    span.observe(&serde_json::from_value(first).unwrap())
        .unwrap();
    span.observe(&serde_json::from_value(second).unwrap())
        .unwrap();
    let rows = rendered(span);
    assert_eq!(rows.last().unwrap()["unclosed"], 2);
}

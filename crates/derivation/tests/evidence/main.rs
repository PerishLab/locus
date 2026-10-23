use locus::Atom;
use locus_derivation::evidence::Evidence;
use locus_derivation::inspect::{Config, Inspection};
use serde_json::json;

fn config() -> Config {
    Config::parse(
        "test",
        r#"
version = 1
disable = ["trace.prefix", "trace.held", "span.size", "span.prefix", "span.held"]
[[analyzer]]
id = "trace.size"
mapping = { kind = "representation-bytes", group = "locus.trace" }
threshold = { above = 0 }
"#,
    )
    .unwrap()
}

fn atom(trace: &str) -> Atom {
    serde_json::from_value(json!({"at": 1, "context": {"locus.trace": trace}, "choices": []}))
        .unwrap()
}

#[test]
fn groups() {
    let config = config();
    let mut inspection = Inspection::new(&config);
    for index in 0..10_001 {
        inspection.observe(&atom(&index.to_string()), 1).unwrap();
    }
    assert!(Evidence::new(inspection.finish().findings()).is_err());
}

#[test]
fn budget() {
    let config = config();
    let atom = atom(&"x".repeat(17 * 1024 * 1024));
    let mut inspection = Inspection::new(&config);
    inspection.observe(&atom, 1).unwrap();
    assert!(Evidence::new(inspection.finish().findings()).is_err());
}

#[test]
fn omissions() {
    let config = config();
    let mut inspection = Inspection::new(&config);
    let mut atom = serde_json::to_value(atom("trace")).unwrap();
    atom["context"]["sample"] = json!("x".repeat(5 * 1024 * 1024));
    let atom: Atom = serde_json::from_value(atom).unwrap();
    inspection.observe(&atom, 1).unwrap();
    let mut evidence = Evidence::new(inspection.finish().findings()).unwrap();
    for _ in 0..5 {
        evidence.observe(&atom, 1).unwrap();
    }
    let values: Vec<_> = evidence
        .finish()
        .map(|group| serde_json::to_value(group).unwrap())
        .collect();
    assert_eq!(values[0]["records"], 5);
    assert_eq!(values[0]["references"].as_array().unwrap().len(), 3);
    assert_eq!(values[0]["omitted"]["references"], 2);
    assert_eq!(values[0]["contributions"][0]["records"], 5);
}

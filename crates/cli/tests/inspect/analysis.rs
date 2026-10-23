use super::fixture;

use fixture::{Root, framed, records, stub};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::process::Output;

fn analyze(root: &Root, input: &str, args: &[&str]) -> Output {
    stub::locus()
        .arg("analyze")
        .arg(root.path())
        .args(["--api", &stub::serve(input)])
        .args(args)
        .output()
        .unwrap()
}

fn input() -> String {
    let mut lines = Vec::new();
    for index in 0..6 {
        for (at, edge) in [(index * 2 + 1, "enter"), (index * 2 + 2, "return")] {
            let mut atom: Value = serde_json::from_str(&framed(at, "work", edge)).unwrap();
            atom["producer"] = json!(if index % 2 == 0 { "alpha" } else { "bravo" });
            atom["payload"] = json!("x".repeat(1000));
            lines.push(atom.to_string());
        }
    }
    format!("{}\n", lines.join("\n"))
}

#[test]
fn sequence() {
    let root = Root::new(None);
    let input = input();
    let output = analyze(&root, &input, &["--token", "reader-secret-sentinel"]);
    assert_eq!(output.status.code(), Some(1), "{:?}", output.stderr);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("reader-secret-sentinel"));
    let analyzed = records(&output);
    for command in ["inspect", "span"] {
        let mut run = stub::locus();
        run.arg(command);
        if command == "inspect" {
            run.arg(root.path());
        }
        let expected = run.args(["--api", &stub::serve(&input)]).output().unwrap();
        let schema = format!("locus.{command}/v1");
        let actual: Vec<_> = analyzed
            .iter()
            .filter(|value| value["schema"] == schema)
            .cloned()
            .collect();
        assert_eq!(actual, records(&expected));
    }
    let summary = analyzed.last().unwrap();
    assert_eq!(summary["schema"], "locus.analyze/v1");
    assert_eq!(summary["records"], 12);
    assert_eq!(summary["bytes"], input.len());
    assert_eq!(
        summary["sha256"],
        format!("{:x}", Sha256::digest(input.as_bytes()))
    );
    assert!(summary["request"].get("token").is_none());
    assert_eq!(
        summary["config"]["analyzers"][0]["mapping"]["kind"],
        "representation-bytes"
    );
    let group = analyzed
        .iter()
        .find(|value| value["schema"] == "locus.evidence/v1" && value["role"] == "locus.trace")
        .unwrap();
    assert_eq!(group["records"], 12);
    assert_eq!(group["bytes"], input.len());
    assert_eq!(group["references"].as_array().unwrap().len(), 4);
    assert_eq!(group["omitted"]["references"], 8);
    assert_eq!(group["contributions"].as_array().unwrap().len(), 4);
    assert_eq!(group["references"][0]["ordinal"], 1);
    assert!(group["references"][0]["id"].is_null());
}

#[test]
fn unknown() {
    let root = Root::new(None);
    let input = format!(
        "{}\n",
        fixture::atom(Some("trace"), json!("x".repeat(9000)))
    );
    let output = analyze(&root, &input, &[]);
    assert_eq!(output.status.code(), Some(1));
    let values = records(&output);
    let group = values
        .iter()
        .find(|value| value["schema"] == "locus.evidence/v1")
        .unwrap();
    assert!(group["contributions"][0]["producer"].is_null());
    assert!(group["contributions"][0]["source"].is_null());
    assert_eq!(group["omitted"]["contributions"], 0);
}

#[test]
fn bounded() {
    let root = Root::new(None);
    let mut lines = Vec::new();
    for index in 0..33 {
        let mut atom: Value =
            serde_json::from_str(&fixture::atom(Some("trace"), json!("x".repeat(300)))).unwrap();
        atom["producer"] = json!(format!("producer-{index}"));
        lines.push(atom.to_string());
    }
    let output = analyze(&root, &format!("{}\n", lines.join("\n")), &[]);
    assert_eq!(output.status.code(), Some(1));
    let values = records(&output);
    let group = values
        .iter()
        .find(|value| value["schema"] == "locus.evidence/v1")
        .unwrap();
    assert_eq!(group["contributions"].as_array().unwrap().len(), 32);
    assert_eq!(group["omitted"]["contributions"], 1);
    assert_eq!(group["omitted"]["references"], 29);
}

#[test]
fn refusal() {
    let root = Root::new(None);
    let input = input();
    for (body, args) in [
        (format!("{input}{{\n"), vec![]),
        (input.trim_end().to_owned(), vec![]),
        (input.clone(), vec!["--max-bytes", "1"]),
        (input.clone(), vec!["--max-records", "1"]),
        (format!("{}\n", "x".repeat(1024 * 1024)), vec![]),
        (
            format!(
                "{}\n{}\n",
                framed(1, "work", "enter"),
                framed(2, "work", "enter")
            ),
            vec![],
        ),
    ] {
        let output = analyze(&root, &body, &args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn empty() {
    let root = Root::new(None);
    let output = analyze(&root, "", &[]);
    assert_eq!(output.status.code(), Some(0));
    let values = records(&output);
    assert_eq!(values.last().unwrap()["records"], 0);
    assert_eq!(values.len(), 3);
}

#[test]
fn transport() {
    let root = Root::new(None);
    let input = input();
    let output = stub::locus()
        .arg("analyze")
        .arg(root.path())
        .args(["--api", &stub::response(&input, input.len() + 100)])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}

#[test]
fn analyzers() {
    let mut config = String::from("version = 1\n");
    for index in 0..65 {
        config.push_str(&format!(
            "\n[[analyzer]]\nid = \"custom-{index}\"\nmapping = {{ kind = \"representation-bytes\", group = \"locus.trace\" }}\nthreshold = {{ above = 1 }}\n"
        ));
    }
    let root = Root::new(Some(&config));
    let output = stub::locus()
        .arg("analyze")
        .arg(root.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("64 effective analyzers"));
}

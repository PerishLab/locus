use locus::Atom;
use locus_derivation::span::Span;
use serde_json::{Value, json};

struct Frame<'a> {
    trace: &'a str,
    span: &'a str,
    function: &'a str,
    parent: Option<&'a str>,
}

fn frame(frame: &Frame<'_>, at: u64, edge: &str) -> Atom {
    let mut context = json!({"locus.trace": frame.trace, "locus.span": frame.span});
    if let Some(parent) = frame.parent {
        context["locus.parent"] = json!(parent);
    }
    let value = json!({
        "at": at,
        "context": context,
        "choices": [],
        "source": {
            "file": "a.rs",
            "line": 1,
            "column": 1,
            "module": "m",
            "function": frame.function,
            "edge": edge,
        },
    });
    serde_json::from_value(value).expect("atom")
}

fn start(trace: &str, parent: &str, at: u64) -> Atom {
    let value = json!({
        "at": at,
        "context": {"locus.trace": trace, "locus.parent": parent},
        "choices": [],
        "payload": {"event": "cli.start"},
    });
    serde_json::from_value(value).expect("atom")
}

fn span<'a>(trace: &'a str, span: &'a str, function: &'a str) -> Frame<'a> {
    Frame {
        trace,
        span,
        function,
        parent: None,
    }
}

fn derive(atoms: &[Atom]) -> Vec<Value> {
    let mut derivation = Span::default();
    let mut ordered: Vec<&Atom> = atoms.iter().collect();
    ordered.sort_by_key(|atom| atom.at());
    for atom in ordered {
        derivation.observe(atom).expect("observe");
    }
    let report = derivation.finish();
    report
        .records()
        .iter()
        .map(|record| serde_json::to_value(record).expect("record"))
        .collect()
}

fn declared(records: &[Value], function: &str) -> Option<Value> {
    records
        .iter()
        .find(|record| record["declaration"] == format!("m::{function}"))
        .cloned()
}

fn summary(records: &[Value]) -> Value {
    records.last().expect("summary").clone()
}

#[test]
fn nested() {
    let outer = span("p", "a", "outer");
    let child = span("c", "x", "child");
    let records = derive(&[
        frame(&outer, 0, "enter"),
        start("c", "a", 5),
        frame(&child, 10, "enter"),
        frame(&child, 40, "return"),
        frame(&outer, 100, "return"),
    ]);
    let parent = declared(&records, "outer").expect("outer");
    assert_eq!(parent["inclusive"], 100);
    assert_eq!(parent["held"], 70);
    assert_eq!(declared(&records, "child").expect("child")["held"], 30);
    let summary = summary(&records);
    assert_eq!(
        summary["links"],
        json!({"nested": 1, "outlived": 0, "missing": 0, "conflicting": 0})
    );
    assert_eq!(summary["refused"], 0);
}

#[test]
fn concurrent() {
    let outer = span("p", "a", "outer");
    let left = Frame {
        parent: Some("a"),
        ..span("l", "x", "worker")
    };
    let right = Frame {
        parent: Some("a"),
        ..span("r", "y", "worker")
    };
    let records = derive(&[
        frame(&outer, 0, "enter"),
        frame(&left, 10, "enter"),
        frame(&right, 30, "enter"),
        frame(&left, 50, "return"),
        frame(&right, 70, "return"),
        frame(&outer, 100, "return"),
    ]);
    assert_eq!(declared(&records, "outer").expect("outer")["held"], 40);
    assert_eq!(declared(&records, "worker").expect("worker")["spans"], 2);
    let summary = summary(&records);
    assert_eq!(summary["links"]["nested"], 2);
    assert_eq!(summary["tangled"], 0);
}

#[test]
fn outlived() {
    let outer = span("p", "a", "outer");
    let child = Frame {
        parent: Some("a"),
        ..span("c", "x", "daemon")
    };
    let records = derive(&[
        frame(&outer, 0, "enter"),
        frame(&child, 50, "enter"),
        frame(&outer, 100, "return"),
        frame(&child, 150, "return"),
    ]);
    assert!(declared(&records, "outer").is_none());
    assert_eq!(declared(&records, "daemon").expect("daemon")["held"], 100);
    let summary = summary(&records);
    assert_eq!(summary["links"]["outlived"], 1);
    assert_eq!(summary["refused"], 1);
}

#[test]
fn missing() {
    let child = Frame {
        parent: Some("gone"),
        ..span("c", "x", "child")
    };
    let records = derive(&[frame(&child, 10, "enter"), frame(&child, 40, "return")]);
    assert_eq!(declared(&records, "child").expect("child")["held"], 30);
    assert_eq!(summary(&records)["links"]["missing"], 1);
}

#[test]
fn levels() {
    let outer = span("p", "a", "outer");
    let middle = Frame {
        parent: Some("a"),
        ..span("c", "x", "middle")
    };
    let inner = Frame {
        parent: Some("x"),
        ..span("g", "z", "inner")
    };
    let records = derive(&[
        frame(&outer, 0, "enter"),
        frame(&middle, 10, "enter"),
        frame(&inner, 20, "enter"),
        frame(&inner, 50, "return"),
        frame(&middle, 60, "return"),
        frame(&outer, 100, "return"),
    ]);
    assert_eq!(declared(&records, "outer").expect("outer")["held"], 50);
    assert_eq!(declared(&records, "middle").expect("middle")["held"], 20);
    assert_eq!(declared(&records, "inner").expect("inner")["held"], 30);
    assert_eq!(summary(&records)["links"]["nested"], 2);
}

#[test]
fn conflicting() {
    let outer = span("p", "a", "outer");
    let child = Frame {
        parent: Some("a"),
        ..span("c", "x", "child")
    };
    let records = derive(&[
        frame(&outer, 0, "enter"),
        start("c", "other", 5),
        frame(&child, 10, "enter"),
        frame(&child, 40, "return"),
        frame(&outer, 100, "return"),
    ]);
    assert_eq!(declared(&records, "outer").expect("outer")["held"], 100);
    assert_eq!(summary(&records)["links"]["conflicting"], 1);
}

#[test]
fn crossing() {
    let outer = span("p", "a", "outer");
    let first = span("p", "b", "first");
    let second = span("p", "d", "second");
    let child = Frame {
        parent: Some("a"),
        ..span("c", "x", "child")
    };
    let records = derive(&[
        frame(&outer, 0, "enter"),
        frame(&child, 10, "enter"),
        frame(&child, 40, "return"),
        frame(&outer, 100, "return"),
        frame(&first, 200, "enter"),
        frame(&second, 210, "enter"),
        frame(&first, 220, "return"),
        frame(&second, 230, "return"),
    ]);
    assert_eq!(declared(&records, "outer").expect("outer")["held"], 70);
    assert!(declared(&records, "first").is_none());
    let summary = summary(&records);
    assert_eq!(summary["links"]["nested"], 1);
    assert_eq!(summary["refused"], 2);
}

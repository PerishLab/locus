#![allow(dead_code)]

use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) const CONFIG: &str = r#"
version = 1

[[analyzer]]
id = "trace.size"
mapping = { kind = "representation-bytes", group = "locus.trace" }
threshold = { above = 8192 }

[[analyzer]]
id = "trace.prefix"
mapping = { kind = "dominant-content-prefix-percent", group = "locus.trace" }
threshold = { above = 80 }
"#;

pub(super) const TIMED: &str = r#"
version = 1

[[analyzer]]
id = "trace.held"
mapping = { kind = "held-time-nanoseconds", group = "locus.trace" }
threshold = { above = 5 }
"#;

static NEXT: AtomicUsize = AtomicUsize::new(0);

pub(super) struct Root(PathBuf);

impl Root {
    pub(super) fn new(config: Option<&str>) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("locus-cli-test-{}-{serial}", std::process::id()));
        fs::create_dir(&path).unwrap();
        if let Some(config) = config {
            fs::write(path.join("locus.toml"), config).unwrap();
        }
        Self(path)
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

pub(super) fn atom(trace: Option<&str>, payload: Value) -> String {
    let mut context = serde_json::Map::new();
    if let Some(trace) = trace {
        context.insert("locus.trace".into(), Value::String(trace.into()));
    }
    json!({
        "at": 1,
        "context": context,
        "choices": [],
        "payload": payload,
    })
    .to_string()
}

pub(super) fn framed(at: u64, span: &str, edge: &str) -> String {
    json!({
        "at": at,
        "context": {"locus.trace": "trace", "locus.span": span},
        "choices": [],
        "source": {
            "file": "a.rs",
            "line": 1,
            "column": 1,
            "module": "m",
            "function": span,
            "edge": edge,
        },
    })
    .to_string()
}

pub(super) fn run(root: &Root, input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_locus"))
        .arg("inspect")
        .arg(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

pub(super) fn records(output: &Output) -> Vec<Value> {
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

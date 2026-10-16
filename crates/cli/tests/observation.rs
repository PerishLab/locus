#[path = "inspect/fixture.rs"]
mod fixture;

use fixture::{Root, atom, stub};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output};

const ENABLED: &str = "[locus]\nenabled = true\nendpoint = 'http://127.0.0.1:0'\n";

struct Fixture {
    home: Root,
    product: Root,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Self {
            home: Root::new(None),
            product: Root::new(None),
        };
        std::fs::create_dir_all(fixture.seat()).unwrap();
        fixture
    }

    fn seat(&self) -> PathBuf {
        self.home
            .path()
            .join(if cfg!(windows) { "locus" } else { ".locus" })
    }

    fn configure(&self, text: &str) {
        std::fs::write(self.seat().join("config.toml"), text).unwrap();
    }

    fn run(&self, args: &[&str], input: &str) -> Output {
        let api = stub::serve(input);
        self.invoke(args, &api)
    }

    fn invoke(&self, args: &[&str], api: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_locus"))
            .current_dir(self.product.path())
            .env("HOME", self.home.path())
            .env("USERPROFILE", self.home.path())
            .env("LOCALAPPDATA", self.home.path())
            .args(args)
            .args(["--api", api])
            .output()
            .unwrap()
    }

    fn records(&self) -> Vec<Value> {
        let mut atoms = Vec::new();
        for entry in std::fs::read_dir(self.seat().join("state/locus")).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy();
            if name == "active.jsonl" || name.starts_with("sealed-") {
                let content = std::fs::read_to_string(path).unwrap();
                atoms.extend(
                    content
                        .lines()
                        .map(|line| serde_json::from_str::<Value>(line).unwrap()),
                );
            }
        }
        atoms
    }
}

fn input() -> String {
    format!(
        "{}\n",
        atom(Some("private-key"), json!({"private": "private-input"}))
    )
}

#[test]
fn disabled() {
    let fixture = Fixture::new();
    let absent = fixture.run(&["query", "locus.trace", "private-key"], &input());
    fixture.configure("[locus]\nenabled = false\n");
    let disabled = fixture.run(&["query", "locus.trace", "private-key"], &input());
    assert_eq!(absent.status.code(), Some(0));
    assert_eq!(absent.status.code(), disabled.status.code());
    assert_eq!(absent.stdout, disabled.stdout);
    assert_eq!(absent.stderr, disabled.stderr);
    assert!(!fixture.seat().join("state/locus").exists());
}

#[test]
fn buffered() {
    let fixture = Fixture::new();
    fixture.configure(ENABLED);
    for args in [
        vec!["query", "locus.trace", "private-key"],
        vec!["span"],
        vec!["inspect"],
    ] {
        let result = fixture.run(&args, &input());
        assert_eq!(
            result.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
    }
    std::fs::write(fixture.product.path().join("locus.toml"), "version = 1\n\n[[analyzer]]\nid = 'trace.size'\nmapping = { kind = 'representation-bytes', group = 'locus.trace' }\nthreshold = { above = 1 }\n").unwrap();
    let finding = fixture.run(&["inspect"], &input());
    assert_eq!(finding.status.code(), Some(1));
    let malformed = fixture.run(&["query", "locus.trace"], "private-malformed\n");
    assert_eq!(malformed.status.code(), Some(2));
    assert!(malformed.stdout.is_empty());
    let unavailable = fixture.invoke(&["span"], "http://127.0.0.1:0");
    assert_eq!(unavailable.status.code(), Some(2));
    let atoms = fixture.records();
    assert!(atoms.iter().all(|atom| atom["producer"] == "locus"));
    for command in ["query", "span", "inspect"] {
        assert!(
            atoms
                .iter()
                .any(|atom| atom["context"]["locus.command"] == command)
        );
    }
    for code in [0, 1, 2] {
        assert!(atoms.iter().any(
            |atom| atom["payload"]["event"] == "cli.finish" && atom["payload"]["code"] == code
        ));
    }
    for edge in ["enter", "return"] {
        assert_eq!(
            atoms
                .iter()
                .filter(|atom| atom["source"]["edge"] == edge)
                .count(),
            23
        );
    }
    let text = serde_json::to_string(&atoms).unwrap();
    for private in [
        "private-key",
        "private-input",
        "private-malformed",
        "127.0.0.1",
    ] {
        assert!(!text.contains(private));
    }
}

#[test]
fn isolated() {
    let fixture = Fixture::new();
    let baseline = fixture.run(&["query", "locus.trace"], &input());
    for config in [
        ENABLED,
        "[locus]\nenabled = 'private-setting'\n",
        "[locus]\nenabled = true\nendpoint = ''\n",
    ] {
        fixture.configure(config);
        let result = fixture.run(&["query", "locus.trace"], &input());
        assert_eq!(result.status.code(), baseline.status.code());
        assert_eq!(result.stdout, baseline.stdout);
        assert!(!result.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("private-setting"));
        let refused = fixture.run(&["span"], "private-malformed\n");
        assert_eq!(refused.status.code(), Some(2));
        assert!(refused.stdout.is_empty());
    }
    let blocked = Fixture::new();
    blocked.configure(ENABLED);
    std::fs::write(blocked.seat().join("state"), "occupied").unwrap();
    let result = blocked.run(&["query", "locus.trace"], &input());
    assert_eq!(result.status.code(), Some(0));
    assert_eq!(result.stdout, baseline.stdout);
    assert!(!result.stderr.is_empty());
}

fn refuse() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut chunk = [0; 1024];
        while !request.ends_with(b"\r\n\r\n") {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => return,
                Ok(read) => request.extend_from_slice(&chunk[..read]),
            }
        }
        let body = r#"{"fault":{"code":"request.unauthorized","message":"private-refusal"}}"#;
        let head = format!(
            "HTTP/1.1 401 Unauthorized\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(body.as_bytes());
    });
    api
}

#[test]
fn faulted() {
    let fixture = Fixture::new();
    let baseline = fixture.invoke(&["span"], &refuse());
    assert_eq!(baseline.status.code(), Some(2));
    fixture.configure(ENABLED);
    let refused = fixture.invoke(&["span"], &refuse());
    assert_eq!(refused.status.code(), baseline.status.code());
    assert_eq!(refused.stdout, baseline.stdout);
    let success = fixture.run(&["span"], &input());
    assert_eq!(success.status.code(), Some(0));
    let unavailable = fixture.invoke(&["span"], "http://127.0.0.1:0");
    assert_eq!(unavailable.status.code(), Some(2));
    let finishes: Vec<Value> = fixture
        .records()
        .into_iter()
        .filter(|atom| atom["payload"]["event"] == "cli.finish")
        .collect();
    assert_eq!(finishes.len(), 3);
    let faults: Vec<&Value> = finishes
        .iter()
        .filter_map(|atom| atom["context"].get("locus.fault"))
        .collect();
    assert_eq!(faults, [&json!("request.unauthorized")]);
    for atom in &finishes {
        if atom["context"].get("locus.fault").is_some() {
            assert_eq!(atom["payload"]["code"], 2);
        }
    }
    let clean: Vec<&Value> = finishes
        .iter()
        .filter(|atom| atom["payload"]["code"] == 0)
        .collect();
    assert_eq!(clean.len(), 1);
    assert!(clean[0]["context"].get("locus.fault").is_none());
    assert!(
        !serde_json::to_string(&finishes)
            .unwrap()
            .contains("private-refusal")
    );
}

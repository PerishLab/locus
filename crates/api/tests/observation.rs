use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Nest(PathBuf);
impl Nest {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "locus-api-observation-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join(".locus")).unwrap();
        Self(root)
    }
    fn seat(&self) -> PathBuf {
        self.0.join(".locus")
    }
    fn configure(&self, text: &str) {
        std::fs::write(self.seat().join("config.toml"), text).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_locus-api"))
            .env_clear()
            .env("HOME", &self.0)
            .env("USERPROFILE", &self.0)
            .env("LOCALAPPDATA", &self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn records(&self) -> Vec<Value> {
        let mut atoms = Vec::new();
        for entry in std::fs::read_dir(self.seat().join("state/locus")).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy();
            if name == "active.jsonl" || name.starts_with("sealed-") {
                atoms.extend(
                    std::fs::read_to_string(path)
                        .unwrap()
                        .lines()
                        .map(|line| serde_json::from_str::<Value>(line).unwrap()),
                );
            }
        }
        atoms
    }
}
impl Drop for Nest {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
const ENABLED: &str = "[locus]\nenabled = true\nendpoint = 'http://127.0.0.1:0'\n";
#[test]
fn buffered() {
    let n = Nest::new();
    let baseline = n.run(&["export-openapi"]);
    assert_eq!(baseline.status.code(), Some(0));
    n.configure(ENABLED);
    let out = n.run(&["export-openapi"]);
    assert_eq!(out.status.code(), baseline.status.code());
    assert_eq!(out.stdout, baseline.stdout);
    let atoms = n.records();
    assert!(atoms.iter().all(|a| a["producer"] == "locus"));
    assert!(
        atoms
            .iter()
            .any(|a| a["context"]["locus.command"] == "api.export-openapi")
    );
    for edge in ["enter", "return"] {
        assert_eq!(
            atoms.iter().filter(|a| a["source"]["edge"] == edge).count(),
            1
        );
    }
    assert_eq!(
        atoms
            .iter()
            .filter(|a| a["payload"]["event"] == "cli.finish" && a["payload"]["code"] == 0)
            .count(),
        1
    );
    let text = serde_json::to_string(&atoms).unwrap();
    assert!(
        atoms
            .iter()
            .all(|a| a["payload"].get("openapi").is_none() && a["payload"].get("paths").is_none())
    );
    assert!(!text.contains(n.0.to_str().unwrap()));
}
#[test]
fn isolated() {
    let n = Nest::new();
    let baseline = n.run(&["export-openapi"]);
    n.configure("[locus]\nenabled = false\n");
    let off = n.run(&["export-openapi"]);
    assert_eq!(
        (off.status.code(), off.stdout, off.stderr),
        (
            baseline.status.code(),
            baseline.stdout.clone(),
            baseline.stderr.clone()
        )
    );
    assert!(!n.seat().join("state/locus").exists());
    for text in [
        ENABLED,
        "[locus]\nenabled = 'private-setting'\n",
        "[locus]\nenabled = true\nendpoint = ''\n",
    ] {
        n.configure(text);
        let out = n.run(&["export-openapi"]);
        assert_eq!(out.status.code(), baseline.status.code());
        assert_eq!(out.stdout, baseline.stdout);
        assert!(!out.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&out.stderr).contains("private-setting"));
    }
    let blocked = Nest::new();
    blocked.configure(ENABLED);
    std::fs::write(blocked.seat().join("state"), "occupied").unwrap();
    let out = blocked.run(&["export-openapi"]);
    assert_eq!(out.status.code(), baseline.status.code());
    assert_eq!(out.stdout, baseline.stdout);
    assert!(!out.stderr.is_empty());
}
#[test]
fn serving() {
    let n = Nest::new();
    n.configure(ENABLED);
    let out = n.run(&["serve", "--listen", "192.0.2.1:43308"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&out.stderr).contains("observation"));
    assert!(!n.seat().join("state/locus").exists());
}

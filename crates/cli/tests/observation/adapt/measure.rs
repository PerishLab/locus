use super::super::fixture::stub;
use super::{atoms, invoke, tool};
use serde_json::Value;

fn inspect(commands: &[&str]) -> (i32, Vec<Value>) {
    let root = tempfile::tempdir().unwrap();
    let config = "version = 1\n[[analyzer]]\nid = 'command.prefix'\nmapping = { kind = 'dominant-content-prefix-percent', group = 'codex.session' }\nthreshold = { above = 80 }\n";
    std::fs::write(root.path().join("locus.toml"), config).unwrap();
    let buffer = root.path().join("buffer");
    for command in commands {
        invoke(&buffer, tool("PreToolUse", command).to_string().as_bytes());
    }
    let selected: Vec<_> = atoms(&buffer)
        .into_iter()
        .filter(|atom| atom["context"]["codex.command"] == "known.v1")
        .collect();
    let input = selected
        .iter()
        .map(|atom| format!("{atom}\n"))
        .collect::<String>();
    let output = stub::locus()
        .arg("analyze")
        .arg(root.path())
        .args(["--api", &stub::serve(&input)])
        .output()
        .unwrap();
    let rows = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (output.status.code().unwrap(), rows)
}

#[test]
fn existing() {
    for repeated in [
        "runseal :seat env GODEBUG=http2client=0 gh api user",
        "gh issue list",
    ] {
        let (exit, rows) = inspect(&[repeated; 6]);
        assert_eq!(exit, 1);
        let finding = rows.iter().find(|row| row["kind"] == "finding").unwrap();
        assert_eq!(finding["measurement"]["atoms"], 6);
        assert_eq!(finding["measurement"]["repeated"], 6);
        assert!(rows.iter().any(|row| row["schema"] == "locus.evidence/v1"));
        assert_eq!(rows.last().unwrap()["schema"], "locus.analyze/v1");
    }
}

#[test]
fn controls() {
    let varied = [
        "gh api user",
        "plumb land",
        "concord member list",
        "santi health",
        "env GODEBUG=http2client=0 gh issue list",
        "runseal :seat gh pr list",
    ];
    assert_eq!(inspect(&varied).0, 0);
    assert_eq!(inspect(&["gh issue list"; 4]).0, 0);
    assert_eq!(
        inspect(&[
            "gh issue list",
            "gh issue list",
            "gh issue list",
            "gh issue list",
            "plumb land"
        ])
        .0,
        0
    );
    let (exit, rows) = inspect(&[
        "gh issue list",
        "gh issue list",
        "gh issue list",
        "gh issue list",
        "gh issue list",
        "plumb land",
    ]);
    assert_eq!(exit, 1);
    let finding = rows.iter().find(|row| row["kind"] == "finding").unwrap();
    assert_eq!(finding["measurement"]["repeated"], 5);
}

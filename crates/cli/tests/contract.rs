use sha2::{Digest, Sha256};
use std::process::Command;

const HELP: &str = "ed8b92b9c8ae2336bbd75487af7dde0064093614ce095963a5511cf2b22d8be2";

#[test]
fn help() {
    let tree = [
        vec!["--help"],
        vec!["deliver", "--help"],
        vec!["inspect", "--help"],
        vec!["analyze", "--help"],
        vec!["query", "--help"],
        vec!["span", "--help"],
        vec!["tree", "--help"],
        vec!["adapt", "--help"],
        vec!["adapt", "codex", "--help"],
        vec!["quality", "--help"],
        vec!["quality", "codex", "--help"],
    ]
    .iter()
    .map(|args| {
        let output = Command::new(env!("CARGO_BIN_EXE_locus"))
            .args(args)
            .output()
            .expect("locus");
        assert!(output.status.success(), "{args:?}");
        String::from_utf8(output.stdout).expect("utf8")
    })
    .collect::<Vec<_>>()
    .join("\n");
    assert_eq!(
        format!("{:x}", Sha256::digest(tree.as_bytes())),
        HELP,
        "help contract moved:\n{tree}"
    );
}

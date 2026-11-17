use sha2::{Digest, Sha256};
use std::process::Command;

const HELP: &str = "cdf2c5179249b1ec4be30dfe014da03563d926784cf086dc5a16838b27721039";

#[test]
fn help() {
    let tree = [
        vec!["--help"],
        vec!["inspect", "--help"],
        vec!["analyze", "--help"],
        vec!["query", "--help"],
        vec!["span", "--help"],
        vec!["tree", "--help"],
        vec!["adapt", "--help"],
        vec!["adapt", "codex", "--help"],
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

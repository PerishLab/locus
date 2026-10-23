use sha2::{Digest, Sha256};
use std::process::Command;

const HELP: &str = "03462d8f41c15a4a86e759a2d3b1eda9d85e3575e5ac0bb8f7dad478938a09aa";

#[test]
fn help() {
    let tree = [
        vec!["--help"],
        vec!["inspect", "--help"],
        vec!["analyze", "--help"],
        vec!["query", "--help"],
        vec!["span", "--help"],
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

use sha2::{Digest, Sha256};
use std::process::Command;

const HELP: &str = "59f3871a4a6a65ede776a4c052afa45764bee7ee002994167dfa5d4c6c3d70f8";

#[test]
fn help() {
    let tree = [
        vec!["--help"],
        vec!["inspect", "--help"],
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

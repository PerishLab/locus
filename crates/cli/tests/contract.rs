use sha2::{Digest, Sha256};
use std::process::Command;

const HELP: &str = "889a566147f00d8b954b736d55f92bbf692b3a19f624cfd005ff653c8878c58a";

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

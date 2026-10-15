use sha2::{Digest, Sha256};
use std::process::Command;

const HELP: &str = "b475afb187fea7515192d89f928748a10ad7dae8d52e949da3cbc167cdf3daa8";

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

use sha2::{Digest, Sha256};
use std::process::Command;

const OPENAPI: &str = "914d9e1717b62ec2b89e9555dd9d44ec3cf7dd4f70a2365bfa0b6ec8f0b0bc1b";
const HELP: &str = "e414a011bf0785b845490f214bc4d8e5fc5d02c883b327bb6960b81dba4fe019";

#[test]
fn openapi() {
    let document = run(&["export-openapi"]);
    assert_eq!(
        digest(&document),
        OPENAPI,
        "OpenAPI contract moved:\n{document}"
    );
}

#[test]
fn help() {
    let tree = [
        vec!["--help"],
        vec!["serve", "--help"],
        vec!["export-openapi", "--help"],
    ]
    .iter()
    .map(|args| run(args))
    .collect::<Vec<_>>()
    .join("\n");
    assert_eq!(digest(&tree), HELP, "help contract moved:\n{tree}");
}

fn run(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_locus-api"))
        .args(args)
        .output()
        .expect("locus-api");
    assert!(output.status.success(), "{args:?}");
    String::from_utf8(output.stdout).expect("utf8")
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

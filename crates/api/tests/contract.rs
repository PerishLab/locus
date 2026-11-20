use sha2::{Digest, Sha256};
use std::process::Command;

const OPENAPI: &str = "6474d449d437db143b18abef0dee20ac36697c222936924a29c0cc1b4190daf8";
const HELP: &str = "9486da4dbe757abe18bee5302cf8d41d69aa7d33c394caa62eddc23a6891559c";

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
        vec!["access", "--help"],
        vec!["export-openapi", "--help"],
        vec!["doctor", "--help"],
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
        .env_clear()
        .env(
            "HOME",
            std::env::temp_dir().join(format!(
                "locus-contract-unconfigured-{}",
                std::process::id()
            )),
        )
        .env(
            "USERPROFILE",
            std::env::temp_dir().join(format!(
                "locus-contract-unconfigured-{}",
                std::process::id()
            )),
        )
        .env(
            "LOCALAPPDATA",
            std::env::temp_dir().join(format!(
                "locus-contract-unconfigured-{}",
                std::process::id()
            )),
        )
        .output()
        .expect("locus-api");
    assert!(output.status.success(), "{args:?}");
    String::from_utf8(output.stdout).expect("utf8")
}

fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

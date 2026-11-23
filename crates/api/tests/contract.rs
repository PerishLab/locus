use sha2::{Digest, Sha256};
use std::process::Command;

const OPENAPI: &str = "3deeded982fd8e789866730f0ac51883ebef93d4d91fb32c09a62d433950138d";
const HELP: &str = "c01b5a445add625dd842aee888e36494bd77b90a88bac38755b103309721e643";

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
        vec!["alerts", "--help"],
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

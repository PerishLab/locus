use sha2::{Digest, Sha256};
use std::process::Command;

const OPENAPI: &str = "960f1c14b8449e3f5b764478c891f12f9ff9fc83d7d90147b89e723c1bfe76fb";
const HELP: &str = "6e95ee6988ec1dc33fffb50b16657a9330b8ebfce2b39826069f0c8e89a2d1a1";

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

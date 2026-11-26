use sha2::{Digest, Sha256};
use std::process::Command;

const OPENAPI: &str = "e1f37943d3f81dc106b17d3cdf84c4b2ee78467c0b08932282434a75ad74801e";
const HELP: &str = "c36637708905a2eae649f5988b6579714e1d92ea14a85b729754734570f21d7e";

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

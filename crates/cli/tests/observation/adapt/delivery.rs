#[cfg(unix)]
#[test]
fn process() {
    use locus_derivation::locus::{Candidate, Config, Context, Engine, Policy, reporter::Spec};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    if let Some(directory) = std::env::var_os("LOCUS_JOURNAL_FIXTURE") {
        let root = std::path::PathBuf::from(directory);
        let spec = Spec::https("https://localhost:9", root.join("key"), root.join("buffer"))
            .sender(root.join("locus"));
        let engine = Engine::bootstrap(Config::new(
            Policy::default().producer("sample").reporter(spec),
        ))
        .unwrap();
        engine
            .append(
                &Context::empty(),
                Candidate::event(serde_json::json!({"fixture":true})),
            )
            .unwrap();
        if std::env::var_os("LOCUS_CRASH_FIXTURE").is_some() {
            std::process::exit(7);
        }
        engine.settle();
        return;
    }
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let path = root.path().join("locus");
    plumb::testing::executable(
        &path,
        r#"#!/bin/sh
test "$1" = deliver && test "$2" = --buffer || exit 3
test -z "${GITHUB_TOKEN+x}${SSH_AUTH_SOCK+x}" || exit 3
test "$SSL_CERT_FILE" = /fixture-ca || exit 3
printf success > "$3/launch"
"#,
    );
    let invoke = || {
        let mut sub = Command::new(std::env::current_exe().unwrap());
        sub.args(["--exact", "adapt::delivery::process"])
            .env("LOCUS_JOURNAL_FIXTURE", root.path())
            .env("GITHUB_TOKEN", "fixture")
            .env("SSH_AUTH_SOCK", "fixture")
            .env("SSL_CERT_FILE", "/fixture-ca");
        sub
    };
    let status = invoke().env("LOCUS_CRASH_FIXTURE", "yes").status().unwrap();
    assert_eq!(status.code(), Some(7));
    let buf = root.path().join("buffer");
    let rows = || {
        let db = rusqlite::Connection::open(buf.join("queue.db")).unwrap();
        db.query_row("SELECT count(*) FROM records", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap()
    };
    assert_eq!(rows(), 1);
    assert!(invoke().status().unwrap().success());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while !buf.join("launch").exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(fs::read(buf.join("launch")).unwrap(), b"success");
    assert_eq!(rows(), 2);
}

use locus::reporter::Spec;
use locus::{Config, Engine, Policy};
use std::path::Path;

fn boot(spec: Spec) -> bool {
    Engine::bootstrap(Config::new(
        Policy::default().producer("sample").reporter(spec),
    ))
    .is_ok()
}

#[test]
fn binding() {
    let root = tempfile::tempdir().unwrap();
    let spool = root.path().join("buffer");
    let key = root.path().join("key");
    for invalid in [
        "http://localhost",
        "https://owner@localhost",
        "https://localhost/path",
        "https://localhost/?q=1",
        "https://localhost/#frag",
    ] {
        assert!(!boot(Spec::https(invalid, &key, &spool)));
    }
    assert!(!boot(Spec::https(
        "https://localhost",
        Path::new("relative"),
        &spool
    )));
    assert!(boot(Spec::https("https://LOCALHOST:443", &key, &spool)));
    assert!(boot(Spec::https("https://localhost/", &key, &spool)));
    assert!(!boot(Spec::https("https://localhost:444", &key, &spool)));
    assert!(!boot(Spec::api("http://127.0.0.1:0", &spool)));
    assert!(locus::reporter::spool::claim(&spool).is_err());
}

#[cfg(unix)]
#[test]
fn encoding() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let invalid = std::path::PathBuf::from(OsString::from_vec(vec![b'/', 255]));
    assert!(!boot(Spec::https("https://localhost", &invalid, &invalid)));
}

#[cfg(unix)]
#[test]
fn helper() {
    use std::fs::{self, Permissions};
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), Permissions::from_mode(0o700)).unwrap();
    let parent = root.path().join("software");
    let home = parent.join("private");
    fs::create_dir_all(&home).unwrap();
    fs::set_permissions(&parent, Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&home, Permissions::from_mode(0o700)).unwrap();
    let binary = home.join("locus");
    fs::copy("/bin/sh", &binary).unwrap();
    let selected = |path: &Path| {
        Spec::https(
            "https://localhost",
            root.path().join("key"),
            root.path().join("queue"),
        )
        .sender(path)
    };
    assert!(boot(selected(&binary)));
    let link = home.join("linked");
    symlink(&binary, &link).unwrap();
    assert!(!boot(selected(&link)));
    fs::set_permissions(&parent, Permissions::from_mode(0o777)).unwrap();
    assert!(!boot(selected(&binary)));
}

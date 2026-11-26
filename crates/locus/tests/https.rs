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

#[cfg(unix)]
#[test]
fn privacy() {
    use std::fs::{self, Permissions};
    use std::os::unix::fs::{PermissionsExt, symlink};
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("key");
    let queue = root.path().join("queue");
    assert!(boot(Spec::https("https://localhost", &key, &queue)));
    let entries = ["delivery.json", "queue.db", "spool.lock"];
    fs::set_permissions(&queue, Permissions::from_mode(0o2770)).unwrap();
    for name in entries {
        fs::set_permissions(queue.join(name), Permissions::from_mode(0o660)).unwrap();
    }
    let error = locus::delivery::send(&queue).unwrap_err();
    assert!(error.contains(&queue.display().to_string()));
    assert!(error.contains("2770"));
    assert_eq!(
        fs::metadata(&queue).unwrap().permissions().mode() & 0o7777,
        0o2770
    );
    assert!(boot(Spec::https("https://localhost", &key, &queue)));
    assert_eq!(
        fs::metadata(&queue).unwrap().permissions().mode() & 0o7777,
        0o0700
    );
    for name in entries {
        assert_eq!(
            fs::metadata(queue.join(name)).unwrap().permissions().mode() & 0o7777,
            0o0600
        );
    }

    for mode in [0o0777, 0o0755] {
        let dir = root.path().join(format!("open-{mode:o}"));
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, Permissions::from_mode(mode)).unwrap();
        let error = Engine::bootstrap(Config::new(
            Policy::default().producer("sample").reporter(Spec::https(
                "https://localhost",
                &key,
                &dir,
            )),
        ))
        .err()
        .unwrap()
        .to_string();
        assert!(error.contains(&dir.display().to_string()));
        assert!(error.contains(&format!("{mode:04o}")));
        assert!(!dir.join("spool.lock").exists());
    }

    let linked = root.path().join("linked");
    symlink(&queue, &linked).unwrap();
    assert!(!boot(Spec::https("https://localhost", &key, &linked)));

    let poisoned = root.path().join("poisoned");
    fs::create_dir(&poisoned).unwrap();
    fs::set_permissions(&poisoned, Permissions::from_mode(0o2770)).unwrap();
    symlink(root.path().join("target"), poisoned.join("delivery.json")).unwrap();
    assert!(!boot(Spec::https("https://localhost", &key, &poisoned)));
    assert_eq!(
        fs::metadata(&poisoned).unwrap().permissions().mode() & 0o7777,
        0o2770
    );
    assert!(!poisoned.join("spool.lock").exists());

    let foreign = Engine::bootstrap(Config::new(
        Policy::default().producer("sample").reporter(Spec::https(
            "https://localhost",
            &key,
            Path::new("/tmp"),
        )),
    ))
    .err()
    .unwrap()
    .to_string();
    assert!(foreign.contains("/tmp"));
    assert!(foreign.contains("owner 0:"));
}

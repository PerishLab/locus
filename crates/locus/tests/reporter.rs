#[cfg(unix)]
mod unix {
    use locus::reporter;
    use locus::{Candidate, Config, Context, Engine, Policy};
    use serde_json::json;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn privacy() {
        let home = temp();
        fs::create_dir(&home).expect("temp");
        let path = home.join("atoms.jsonl");
        let policy = Policy::default().reporter(reporter::Spec::file(&path));
        let engine = Engine::bootstrap(Config::new(policy)).expect("bootstrap");

        engine
            .append(
                &Context::empty(),
                Candidate::event(json!({"event": "private"})),
            )
            .expect("append");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        fs::remove_dir_all(home).expect("cleanup");
    }

    fn temp() -> PathBuf {
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("locus-report-{}-{at}", std::process::id()))
    }
}

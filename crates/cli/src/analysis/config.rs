use locus_derivation::inspect::{Config, File};
use std::fs;
use std::io;
use std::path::Path;

pub(crate) fn read(root: &Path) -> Result<Config, String> {
    if !root.is_dir() {
        return Err(format!("cannot read {}: not a directory", root.display()));
    }
    let plumb = root.join("plumb.toml");
    let section =
        radicula_config::section::<File>(&plumb, "locus").map_err(|error| error.to_string())?;
    let legacy = root.join("locus.toml");
    let content = match fs::read_to_string(&legacy) {
        Ok(content) => Some(content),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("cannot read {}: {error}", legacy.display())),
    };
    match (section, content) {
        (Some(_), Some(_)) => Err(format!(
            "cannot load both {} [locus] and {}; keep only one",
            plumb.display(),
            legacy.display()
        )),
        (Some(file), None) => file.build(&format!("{} [locus]", plumb.display())),
        (None, Some(content)) => Config::parse(&legacy.display().to_string(), &content),
        (None, None) => Ok(Config::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::read;
    use serde_json::json;
    use std::fs;
    use std::path::Path;

    fn write(dir: &Path, name: &str, text: &str) {
        fs::write(dir.join(name), text).unwrap();
    }

    #[test]
    fn section() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "plumb.toml",
            r#"
[identity]
ns = "demo"

[locus]
version = 1
disable = ["span.prefix"]

[[locus.analyzer]]
id = "span.held"
mapping = { kind = "held-time-nanoseconds", group = "locus.span" }
threshold = { above = 10 }

[[locus.analyzer]]
id = "cycle.held"
mapping = { kind = "held-time-nanoseconds", group = "agent.cycle" }
threshold = { above = 1 }
"#,
        );
        let config = read(dir.path()).unwrap();
        assert_eq!(
            config.coverage(),
            vec![
                "trace.size",
                "trace.prefix",
                "trace.held",
                "span.size",
                "span.held",
                "cycle.held",
            ]
        );
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(value["defaults"]["span.prefix"], json!("disabled"));
        assert_eq!(value["defaults"]["span.held"], json!("overridden"));
        assert_eq!(value["defaults"]["trace.size"], json!("default"));
    }

    #[test]
    fn legacy() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "locus.toml",
            "version = 1\ndisable = [\"trace.size\"]\n",
        );
        let config = read(dir.path()).unwrap();
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(value["defaults"]["trace.size"], json!("disabled"));
        assert_eq!(value["defaults"]["span.held"], json!("default"));
    }

    #[test]
    fn sibling() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "plumb.toml", "[identity]\nns = \"demo\"\n");
        write(
            dir.path(),
            "locus.toml",
            "version = 1\ndisable = [\"trace.held\"]\n",
        );
        let config = read(dir.path()).unwrap();
        let value = serde_json::to_value(&config).unwrap();
        assert_eq!(value["defaults"]["trace.held"], json!("disabled"));
    }

    #[test]
    fn conflict() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "plumb.toml", "[locus]\nversion = 1\n");
        write(dir.path(), "locus.toml", "version = 1\n");
        let error = read(dir.path()).err().unwrap();
        assert!(error.contains("plumb.toml [locus]"));
        assert!(error.contains("locus.toml"));
        assert!(error.contains("keep only one"));
    }

    #[test]
    fn plain() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "plumb.toml", "[identity]\nns = \"demo\"\n");
        let config = read(dir.path()).unwrap();
        assert_eq!(
            serde_json::to_value(&config).unwrap(),
            serde_json::to_value(locus_derivation::inspect::Config::default()).unwrap()
        );
    }

    #[test]
    fn empty() {
        let dir = tempfile::tempdir().unwrap();
        let config = read(dir.path()).unwrap();
        assert_eq!(
            serde_json::to_value(&config).unwrap(),
            serde_json::to_value(locus_derivation::inspect::Config::default()).unwrap()
        );
    }

    #[test]
    fn malformed() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "plumb.toml", "[broken\n");
        let error = read(dir.path()).err().unwrap();
        assert!(error.contains("cannot parse"));
    }

    #[test]
    fn unreadable() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("plumb.toml")).unwrap();
        let error = read(dir.path()).err().unwrap();
        assert!(error.contains("cannot read"));
    }

    #[test]
    fn scalar() {
        for value in ["locus = 1\n", "locus = \"bad\"\n"] {
            let dir = tempfile::tempdir().unwrap();
            write(dir.path(), "plumb.toml", value);
            let error = read(dir.path()).err().unwrap();
            assert!(error.contains("not a table"));
        }
    }

    #[test]
    fn unknown() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "plumb.toml",
            "[locus]\nversion = 1\nextra = true\n",
        );
        let error = read(dir.path()).err().unwrap();
        assert!(error.contains("unknown field `extra`"));
    }

    #[test]
    fn version() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "plumb.toml", "[locus]\nversion = 2\n");
        let error = read(dir.path()).err().unwrap();
        assert!(error.contains("unsupported version 2"));
    }

    #[test]
    fn invalid() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "plumb.toml",
            "[locus]\nversion = 1\ndisable = [\"cycle.held\"]\n",
        );
        let error = read(dir.path()).err().unwrap();
        assert!(error.contains("cannot disable unknown default analyzer: cycle.held"));
    }
}

use locus::Atom;
use locus_derivation::inspect::{Config, Inspection};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn config() -> Config {
    Config::parse(
        "test",
        r#"
version = 1
disable = ["trace.size", "trace.held", "span.size", "span.prefix", "span.held"]
[[analyzer]]
id = "trace.prefix"
mapping = { kind = "dominant-content-prefix-percent", group = "locus.trace" }
threshold = { above = 0 }
"#,
    )
    .unwrap()
}

fn expected(contents: &[Option<&str>]) -> Option<Value> {
    if contents.len() < 5 {
        return None;
    }
    let mut prefixes = BTreeMap::<Vec<u8>, u64>::new();
    for content in contents.iter().flatten() {
        for length in 8..=content.len().min(64) {
            *prefixes
                .entry(content.as_bytes()[..length].to_vec())
                .or_default() += 1;
        }
    }
    prefixes
        .into_iter()
        .map(|(prefix, count)| (count, prefix.len()))
        .max_by_key(|(count, length)| (*count, *length))
        .map(|(repeated, bytes)| {
            json!({
                "mapping": "dominant-content-prefix-percent",
                "value": repeated * 100 / contents.len() as u64,
                "atoms": contents.len(),
                "repeated": repeated,
                "bytes": bytes,
            })
        })
}

fn contents(count: usize, mask: usize) -> Vec<Option<&'static str>> {
    (0..count)
        .map(|index| {
            if mask & (1 << index) == 0 {
                None
            } else if index % 2 == 0 {
                Some("abcdefgh-X")
            } else {
                Some("abcdefgh-Y")
            }
        })
        .collect()
}

fn measured(config: &Config, contents: &[Option<&str>]) -> Option<Value> {
    let mut inspection = Inspection::new(config);
    for content in contents {
        let mut atom = json!({"at": 1, "context": {"locus.trace": "trace"}, "choices": []});
        if let Some(content) = content {
            atom["payload"] = json!(content);
        }
        let atom: Atom = serde_json::from_value(atom).unwrap();
        inspection.observe(&atom, 1).unwrap();
    }
    inspection
        .finish()
        .records()
        .map(|record| serde_json::to_value(record).unwrap())
        .find(|record| record["kind"] == "finding")
        .map(|record| record["measurement"].clone())
}

#[test]
fn deferred() {
    let config = config();
    for count in 0..=9 {
        for mask in 0..(1 << count) {
            let contents = contents(count, mask);
            assert_eq!(
                measured(&config, &contents),
                expected(&contents),
                "count={count} mask={mask}"
            );
        }
    }
}

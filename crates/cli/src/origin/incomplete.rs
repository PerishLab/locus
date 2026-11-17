use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

const HORIZON: u64 = 75_000_000_000;

pub struct Bounds {
    pub from: Option<u64>,
    pub to: Option<u64>,
    pub retained: Option<u64>,
}

impl Bounds {
    pub fn mark(&self, encoded: String) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
            });
        let causes = match serde_json::from_str(&encoded) {
            Ok(summary) => self.causes(&summary, now),
            Err(_) => Vec::new(),
        };
        match encoded.strip_suffix('}') {
            Some(open) if !causes.is_empty() => {
                format!("{open},\"incomplete\":{}}}", json!(causes))
            }
            _ => encoded,
        }
    }

    fn causes(&self, summary: &Value, now: u64) -> Vec<&'static str> {
        let headless = count(summary, "headless");
        let unclosed = count(summary, "unclosed");
        let missing = summary
            .get("links")
            .map_or(0, |links| count(links, "missing"));
        let opening = headless + missing > 0;
        let closing = unclosed + missing > 0;
        let retention = self
            .retained
            .is_some_and(|retained| self.from.is_none_or(|from| from < retained));
        let recent = self.to.is_none_or(|to| to > now.saturating_sub(HORIZON));
        [
            ("window.from", self.from.is_some() && opening),
            ("window.to", self.to.is_some() && closing),
            ("retention", retention && opening),
            ("spool", recent && closing),
        ]
        .into_iter()
        .filter_map(|(cause, holds)| holds.then_some(cause))
        .collect()
    }
}

fn count(value: &Value, name: &str) -> u64 {
    value.get(name).and_then(Value::as_u64).unwrap_or(0)
}

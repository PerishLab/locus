use crate::store::Producer;
use locus_derivation::metric::{Kind, Metric, Query};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use url::Url;

pub(super) const SECOND: u64 = 1_000_000_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Rules {
    pub interval: u64,
    #[serde(default)]
    pub delay: u64,
    pub origin: Url,
    pub rules: Vec<Rule>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Rule {
    pub name: String,
    pub revision: u64,
    pub subject: String,
    pub producer: String,
    pub metric: String,
    pub unit: String,
    pub kind: Kind,
    pub above: f64,
    pub hold: u64,
    pub mute: Option<u64>,
    pub recipients: Vec<Recipient>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Recipient {
    pub subject: String,
    pub endpoint: Url,
}

impl Rules {
    pub fn read(home: &Path) -> Result<Option<Self>, String> {
        let raw = match fs::read(home.join("alerts.json")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.to_string()),
        };
        if raw.len() > 65536 {
            return Err("alert declaration exceeds 64 KiB".into());
        }
        let rules: Self = serde_json::from_slice(&raw).map_err(|error| error.to_string())?;
        rules.check()?;
        Ok(Some(rules))
    }

    fn check(&self) -> Result<(), String> {
        address(&self.origin)?;
        if self.origin.path() != "/" || !(1..=300).contains(&self.interval) || self.delay > 300 {
            return Err(
                "alert origin must be bare, interval 1–300 seconds and delay at most 300 seconds"
                    .into(),
            );
        }
        if self.rules.is_empty() || self.rules.len() > 32 {
            return Err("declare 1–32 alerts".into());
        }
        let mut names = BTreeSet::new();
        for rule in &self.rules {
            rule.check(self.interval)?;
            if !names.insert(rule.name.as_str()) {
                return Err("duplicate alert name".into());
            }
        }
        Ok(())
    }
}

impl Rule {
    pub fn query(&self, from: u64, to: u64) -> Query {
        Query {
            producer: self.producer.clone(),
            name: self.metric.clone(),
            unit: self.unit.clone(),
            kind: self.kind,
            from,
            to,
            step: to - from,
            group: None,
        }
    }

    pub fn fingerprint(&self, interval: u64, delay: u64) -> String {
        let mut record = serde_json::to_value(self).expect("valid alert definition");
        record.as_object_mut().expect("alert object").remove("mute");
        let data = serde_json::to_vec(&(interval, delay, record)).expect("valid alert definition");
        format!("{:x}", Sha256::digest(data))
    }

    fn check(&self, interval: u64) -> Result<(), String> {
        Producer::new(&self.producer)?;
        if !identifier(&self.name) || self.revision == 0 || !subject(&self.subject) {
            return Err("invalid alert name, revision, or evaluator subject".into());
        }
        if !self.above.is_finite() || self.hold > 86_400 || self.recipients.len() > 8 {
            return Err("invalid alert threshold, hold, or recipient count".into());
        }
        Metric::new(self.query(0, interval * SECOND))?;
        let mut people = BTreeSet::new();
        for recipient in &self.recipients {
            address(&recipient.endpoint)?;
            if !subject(&recipient.subject) || !people.insert(recipient.subject.as_str()) {
                return Err("invalid or duplicate alert recipient".into());
            }
        }
        Ok(())
    }
}

fn subject(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn address(url: &Url) -> Result<(), String> {
    let local = matches!(url.host(), Some(url::Host::Ipv4(ip)) if ip.is_loopback())
        || matches!(url.host(), Some(url::Host::Ipv6(ip)) if ip.is_loopback());
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        return Err("alerts require HTTPS or an HTTP loopback IP".into());
    }
    if !url.username().is_empty() || url.password().is_some() || !url.has_host() {
        return Err("alert address must have a host and no user credentials".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("alert address must have no query or fragment".into());
    }
    Ok(())
}

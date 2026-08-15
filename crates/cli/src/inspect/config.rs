use super::mapping::Mapping;
use super::model::Threshold;
use locus::Role;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u16,
    #[serde(default)]
    analyzer: Vec<Spec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    id: String,
    mapping: Shape,
    threshold: Threshold,
}

#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Shape {
    #[serde(rename = "representation-bytes")]
    Bytes { group: Role },
    #[serde(rename = "dominant-content-prefix-percent")]
    Prefix { group: Role },
    #[serde(rename = "held-time-nanoseconds")]
    Held { group: Role },
}

pub struct Analyzer {
    pub id: String,
    pub mapping: Mapping,
    pub threshold: Threshold,
}

pub struct Config {
    analyzers: Vec<Analyzer>,
}

impl Config {
    pub fn read(root: &Path) -> Result<Self, String> {
        let path = root.join("locus.toml");
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let file: File = toml::from_str(&content)
            .map_err(|error| format!("invalid {}: {error}", path.display()))?;
        if file.version != 1 {
            return Err(format!(
                "invalid {}: unsupported version {}",
                path.display(),
                file.version
            ));
        }
        let mut identities = BTreeSet::new();
        let mut analyzers = Vec::with_capacity(file.analyzer.len());
        for spec in file.analyzer {
            validate(&spec.id)?;
            if !identities.insert(spec.id.clone()) {
                return Err(format!("duplicate analyzer identity: {}", spec.id));
            }
            analyzers.push(Analyzer {
                id: spec.id,
                mapping: spec.mapping.into(),
                threshold: spec.threshold,
            });
        }
        Ok(Self { analyzers })
    }

    pub fn analyzers(&self) -> &[Analyzer] {
        &self.analyzers
    }

    pub fn coverage(&self) -> Vec<String> {
        self.analyzers
            .iter()
            .map(|analyzer| analyzer.id.clone())
            .collect()
    }
}

impl From<Shape> for Mapping {
    fn from(spec: Shape) -> Self {
        match spec {
            Shape::Bytes { group } => Self::Bytes { group },
            Shape::Prefix { group } => Self::Prefix { group },
            Shape::Held { group } => Self::Held { group },
        }
    }
}

fn validate(identity: &str) -> Result<(), String> {
    enum Fault {
        Empty,
        Long,
        Control,
        Whitespace,
    }
    let fault = if identity.is_empty() {
        Some(Fault::Empty)
    } else if identity.len() > 128 {
        Some(Fault::Long)
    } else if identity.chars().any(char::is_control) {
        Some(Fault::Control)
    } else if identity.chars().any(char::is_whitespace) {
        Some(Fault::Whitespace)
    } else {
        None
    };
    match fault {
        None => Ok(()),
        Some(_) => Err("analyzer identity must contain 1..=128 non-whitespace bytes".into()),
    }
}

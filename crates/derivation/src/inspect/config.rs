use super::mapping::Mapping;
use super::model::Threshold;
use locus::Role;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const SECOND: u64 = 1_000_000_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u16,
    #[serde(default)]
    disable: Vec<String>,
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

impl Spec {
    fn new(id: &str, mapping: Shape, above: u64) -> Self {
        Self {
            id: id.into(),
            mapping,
            threshold: Threshold { above },
        }
    }
}

#[derive(Deserialize, PartialEq)]
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

impl From<Spec> for Analyzer {
    fn from(spec: Spec) -> Self {
        Self {
            id: spec.id,
            mapping: spec.mapping.into(),
            threshold: spec.threshold,
        }
    }
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Default,
    Overridden,
    Disabled,
}

pub struct Config {
    analyzers: Vec<Analyzer>,
    defaults: BTreeMap<String, Origin>,
}

impl Default for Config {
    fn default() -> Self {
        let mut config = Self {
            analyzers: Vec::new(),
            defaults: BTreeMap::new(),
        };
        for spec in defaults() {
            config.defaults.insert(spec.id.clone(), Origin::Default);
            config.analyzers.push(spec.into());
        }
        config
    }
}

impl Config {
    pub fn parse(name: &str, content: &str) -> Result<Self, String> {
        let file: File =
            toml::from_str(content).map_err(|error| format!("invalid {name}: {error}"))?;
        if file.version != 1 {
            return Err(format!(
                "invalid {name}: unsupported version {}",
                file.version
            ));
        }
        compose(file).map_err(|error| format!("invalid {name}: {error}"))
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

    pub fn defaults(&self) -> &BTreeMap<String, Origin> {
        &self.defaults
    }
}

fn defaults() -> Vec<Spec> {
    [
        ("trace", Role::trace(), 300 * SECOND),
        ("span", Role::span(), 5 * SECOND),
    ]
    .into_iter()
    .flat_map(|(scope, group, held)| {
        [
            Spec::new(
                &format!("{scope}.size"),
                Shape::Bytes {
                    group: group.clone(),
                },
                8192,
            ),
            Spec::new(
                &format!("{scope}.prefix"),
                Shape::Prefix {
                    group: group.clone(),
                },
                80,
            ),
            Spec::new(&format!("{scope}.held"), Shape::Held { group }, held),
        ]
    })
    .collect()
}

fn compose(file: File) -> Result<Config, String> {
    let builtin = defaults();
    let disabled = disabled(file.disable, &builtin)?;
    let mut identities = BTreeSet::new();
    let mut overrides = BTreeMap::new();
    let mut additions = Vec::new();
    for spec in file.analyzer {
        validate(&spec.id)?;
        if !identities.insert(spec.id.clone()) {
            return Err(format!("duplicate analyzer identity: {}", spec.id));
        }
        if builtin.iter().any(|default| default.id == spec.id) {
            overrides.insert(spec.id.clone(), spec);
        } else {
            additions.push(spec);
        }
    }
    let mut config = Config {
        analyzers: Vec::new(),
        defaults: BTreeMap::new(),
    };
    for default in builtin {
        let (origin, spec) = resolve(default, &disabled, &mut overrides)?;
        config.defaults.insert(spec.id.clone(), origin);
        if !matches!(origin, Origin::Disabled) {
            config.analyzers.push(spec.into());
        }
    }
    config
        .analyzers
        .extend(additions.into_iter().map(Analyzer::from));
    Ok(config)
}

fn disabled(names: Vec<String>, builtin: &[Spec]) -> Result<BTreeSet<String>, String> {
    let mut disabled = BTreeSet::new();
    for name in names {
        if !builtin.iter().any(|default| default.id == name) {
            return Err(format!("cannot disable unknown default analyzer: {name}"));
        }
        if !disabled.insert(name.clone()) {
            return Err(format!("duplicate disabled analyzer: {name}"));
        }
    }
    Ok(disabled)
}

fn resolve(
    default: Spec,
    disabled: &BTreeSet<String>,
    overrides: &mut BTreeMap<String, Spec>,
) -> Result<(Origin, Spec), String> {
    let own = overrides.remove(&default.id);
    if disabled.contains(&default.id) {
        if own.is_some() {
            return Err(format!(
                "analyzer {} is both disabled and declared",
                default.id
            ));
        }
        return Ok((Origin::Disabled, default));
    }
    match own {
        None => Ok((Origin::Default, default)),
        Some(spec) if spec.mapping == default.mapping => Ok((Origin::Overridden, spec)),
        Some(spec) => Err(format!(
            "analyzer {} must keep its default mapping",
            spec.id
        )),
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

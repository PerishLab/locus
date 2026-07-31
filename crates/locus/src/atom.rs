use crate::{Context, Key, Role};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    Enter,
    Return,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Explicit,
    Generated,
    Inherited,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    role: String,
    key: String,
    origin: Origin,
}

impl Choice {
    pub fn role(&self) -> &str {
        &self.role
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    pub(crate) fn new(role: &Role, key: &Key, origin: Origin) -> Self {
        Self {
            role: role.text().into(),
            key: key.text().into(),
            origin,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    role: String,
    binding: String,
    collector: String,
    selector: String,
}

impl Collection {
    pub fn role(&self) -> &str {
        &self.role
    }

    pub fn binding(&self) -> &str {
        &self.binding
    }

    pub fn collector(&self) -> &str {
        &self.collector
    }

    pub fn selector(&self) -> &str {
        &self.selector
    }

    pub(crate) fn new(role: &Role, binding: &str, collector: &str, selector: String) -> Self {
        Self {
            role: role.text().into(),
            binding: binding.into(),
            collector: collector.into(),
            selector,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Source {
    file: String,
    line: u32,
    column: u32,
    module: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    function: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    edge: Option<Edge>,
}

impl Source {
    pub fn code(file: &str, line: u32, column: u32, module: &str) -> Self {
        Self {
            file: file.into(),
            line,
            column,
            module: module.into(),
            function: None,
            edge: None,
        }
    }

    pub fn enter(mut self, function: &str) -> Self {
        self.function = Some(function.into());
        self.edge = Some(Edge::Enter);
        self
    }

    pub fn returned(mut self, function: &str) -> Self {
        self.function = Some(function.into());
        self.edge = Some(Edge::Return);
        self
    }

    pub fn file(&self) -> &str {
        &self.file
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    pub fn column(&self) -> u32 {
        self.column
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    pub fn function(&self) -> Option<&str> {
        self.function.as_deref()
    }

    pub fn edge(&self) -> Option<&Edge> {
        self.edge.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Atom {
    at: u64,
    context: BTreeMap<String, String>,
    choices: Vec<Choice>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    collections: Vec<Collection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
}

impl Atom {
    pub fn at(&self) -> u64 {
        self.at
    }

    pub fn context(&self) -> &BTreeMap<String, String> {
        &self.context
    }

    pub fn choices(&self) -> &[Choice] {
        &self.choices
    }

    pub fn collections(&self) -> &[Collection] {
        &self.collections
    }

    pub fn source(&self) -> Option<&Source> {
        self.source.as_ref()
    }

    pub fn payload(&self) -> Option<&Value> {
        self.payload.as_ref()
    }

    pub(crate) fn new(
        at: u64,
        context: &Context,
        choices: Vec<Choice>,
        candidate: Candidate,
    ) -> Self {
        Self {
            at,
            context: context.snapshot(),
            choices,
            collections: candidate.provenance,
            source: candidate.source,
            payload: candidate.payload,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Candidate {
    pub(crate) needs: BTreeMap<Role, Option<Key>>,
    pub(crate) provenance: Vec<Collection>,
    pub(crate) requests: Vec<(Role, String)>,
    pub(crate) source: Option<Source>,
    pub(crate) payload: Option<Value>,
}

impl Candidate {
    pub fn context() -> Self {
        Self::default()
    }

    pub fn event(payload: Value) -> Self {
        Self {
            payload: Some(payload),
            ..Self::default()
        }
    }

    pub fn ensure(mut self, role: Role) -> Self {
        self.needs.entry(role).or_insert(None);
        self
    }

    pub fn explicit(mut self, role: Role, key: Key) -> Self {
        self.needs.insert(role, Some(key));
        self
    }

    pub fn collect(mut self, role: Role, binding: impl Into<String>) -> Self {
        self.requests.push((role, binding.into()));
        self
    }

    pub fn source(mut self, source: Source) -> Self {
        self.source = Some(source);
        self
    }
}

#[derive(Clone, Debug)]
pub struct Accepted {
    context: Context,
}

impl Accepted {
    pub(crate) fn new(context: Context) -> Self {
        Self { context }
    }

    pub fn context(&self) -> Context {
        self.context.clone()
    }
}

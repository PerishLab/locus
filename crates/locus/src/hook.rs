use crate::Atom;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Delivered,
    Failed,
    Gated,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outcome {
    atom: Atom,
    reporter: Option<String>,
    status: Status,
    error: Option<String>,
}

impl Outcome {
    pub fn atom(&self) -> &Atom {
        &self.atom
    }

    pub fn reporter(&self) -> Option<&str> {
        self.reporter.as_deref()
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub(crate) fn new(
        atom: Atom,
        reporter: Option<String>,
        status: Status,
        error: Option<String>,
    ) -> Self {
        Self {
            atom,
            reporter,
            status,
            error,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    code: String,
    message: String,
}

impl Diagnostic {
    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Observation {
    Diagnostic(Diagnostic),
    Report(Outcome),
}

pub trait Hook: Send + Sync {
    fn observe(&self, observation: &Observation);
}

#[derive(Clone, Debug, Default)]
pub struct Adaptor;

impl Hook for Adaptor {
    fn observe(&self, observation: &Observation) {
        if let Observation::Diagnostic(diagnostic) = observation {
            eprintln!(
                "locus engine diagnostic [{}]: {}",
                diagnostic.code(),
                diagnostic.message()
            );
        }
    }
}

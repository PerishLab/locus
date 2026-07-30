use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Kind {
    Config,
    Context,
    Generator,
    Record,
    Reporter,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    kind: Kind,
    message: String,
}

impl Error {
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn new(kind: Kind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(crate) fn config(message: impl Into<String>) -> Self {
        Self::new(Kind::Config, message)
    }

    pub(crate) fn context(message: impl Into<String>) -> Self {
        Self::new(Kind::Context, message)
    }

    pub(crate) fn generator(message: impl Into<String>) -> Self {
        Self::new(Kind::Generator, message)
    }

    pub(crate) fn record(message: impl Into<String>) -> Self {
        Self::new(Kind::Record, message)
    }

    pub(crate) fn reporter(message: impl Into<String>) -> Self {
        Self::new(Kind::Reporter, message)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, form: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(form, "{}", self.message)
    }
}

impl std::error::Error for Error {}

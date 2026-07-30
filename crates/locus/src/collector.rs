use crate::Error;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::ffi::OsString;

const LIMIT: usize = 4096;
const ARGUMENTS: usize = 255;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spec {
    name: String,
    options: Value,
}

impl Spec {
    pub fn new(name: impl Into<String>, options: Value) -> Self {
        Self {
            name: name.into(),
            options,
        }
    }

    pub fn environment(name: impl Into<String>, limit: usize) -> Self {
        Self::new("environment", json!({"name": name.into(), "limit": limit}))
    }

    pub fn argument(index: usize, limit: usize) -> Self {
        Self::new("argv", json!({"index": index, "limit": limit}))
    }

    pub fn process(selector: impl Into<String>, limit: usize) -> Self {
        Self::new(
            "process",
            json!({"selector": selector.into(), "limit": limit}),
        )
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn options(&self) -> &Value {
        &self.options
    }
}

pub(crate) struct Builtin {
    name: &'static str,
    selector: String,
    inner: Box<dyn Collect>,
}

pub(crate) struct Sample {
    pub(crate) collector: &'static str,
    pub(crate) selector: String,
    pub(crate) value: String,
}

pub(crate) fn first(chain: &[Builtin]) -> Result<Option<Sample>, Error> {
    for builtin in chain {
        if let Some(value) = builtin.inner.collect()? {
            return Ok(Some(Sample {
                collector: builtin.name,
                selector: builtin.selector.clone(),
                value,
            }));
        }
    }
    Ok(None)
}

trait Collect: Send + Sync {
    fn collect(&self) -> Result<Option<String>, Error>;
}

struct Environment {
    name: String,
    limit: usize,
}

impl Collect for Environment {
    fn collect(&self) -> Result<Option<String>, Error> {
        std::env::var_os(&self.name)
            .map(|value| bounded("environment", value, self.limit))
            .transpose()
    }
}

struct Argument {
    index: usize,
    limit: usize,
}

impl Collect for Argument {
    fn collect(&self) -> Result<Option<String>, Error> {
        std::env::args_os()
            .skip(1)
            .nth(self.index)
            .map(|value| bounded("argv", value, self.limit))
            .transpose()
    }
}

enum Selector {
    Directory,
    Executable,
    Id,
}

impl Selector {
    fn read(&self) -> Result<OsString, Error> {
        match self {
            Self::Directory => std::env::current_dir()
                .map(|path| path.into_os_string())
                .map_err(|error| Error::collector(format!("process directory failed: {error}"))),
            Self::Executable => std::env::current_exe()
                .map(|path| path.into_os_string())
                .map_err(|error| Error::collector(format!("process executable failed: {error}"))),
            Self::Id => Ok(std::process::id().to_string().into()),
        }
    }
}

struct Process {
    selector: Selector,
    limit: usize,
}

impl Collect for Process {
    fn collect(&self) -> Result<Option<String>, Error> {
        bounded("process", self.selector.read()?, self.limit).map(Some)
    }
}

pub(crate) fn bindings(
    specs: &BTreeMap<String, Vec<Spec>>,
) -> Result<BTreeMap<String, Vec<Builtin>>, Error> {
    specs
        .iter()
        .map(|(binding, chain)| {
            validate(binding)?;
            if chain.is_empty() {
                return Err(Error::config(format!(
                    "collector binding {binding} contains no algorithm"
                )));
            }
            let built = chain.iter().map(build).collect::<Result<Vec<_>, _>>()?;
            Ok((binding.clone(), built))
        })
        .collect()
}

fn build(spec: &Spec) -> Result<Builtin, Error> {
    let fields: &[&str] = match spec.name() {
        "environment" => &["limit", "name"],
        "argv" => &["index", "limit"],
        "process" => &["limit", "selector"],
        other => return Err(Error::config(format!("unknown collector: {other}"))),
    };
    let options = Options::new(spec, fields)?;
    let (name, selector, inner): (&'static str, String, Box<dyn Collect>) = match spec.name() {
        "environment" => {
            let selector = options.text("name")?;
            let collector = Environment {
                name: selector.clone(),
                limit: limit(options.number("limit")?)?,
            };
            ("environment", selector, Box::new(collector))
        }
        "argv" => {
            let index = index(options.number("index")?)?;
            let collector = Argument {
                index,
                limit: limit(options.number("limit")?)?,
            };
            ("argv", index.to_string(), Box::new(collector))
        }
        "process" => {
            let selector = options.text("selector")?;
            let collector = Process {
                selector: self::selector(&selector)?,
                limit: limit(options.number("limit")?)?,
            };
            ("process", selector, Box::new(collector))
        }
        _ => unreachable!(),
    };
    Ok(Builtin {
        name,
        selector,
        inner,
    })
}

struct Options<'a> {
    spec: &'a Spec,
    object: &'a Map<String, Value>,
}

impl<'a> Options<'a> {
    fn new(spec: &'a Spec, fields: &[&str]) -> Result<Self, Error> {
        let object = spec
            .options()
            .as_object()
            .ok_or_else(|| Error::config("collector options must be an object"))?;
        let mut held: Vec<&str> = object.keys().map(String::as_str).collect();
        held.sort_unstable();
        if held != fields {
            return Err(Error::config("collector options do not match its schema"));
        }
        Ok(Self { spec, object })
    }

    fn text(&self, name: &str) -> Result<String, Error> {
        self.object
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| Error::config(format!("{} collector requires {name}", self.spec.name())))
    }

    fn number(&self, name: &str) -> Result<u64, Error> {
        self.object
            .get(name)
            .and_then(Value::as_u64)
            .ok_or_else(|| Error::config(format!("{} collector requires {name}", self.spec.name())))
    }
}

fn index(value: u64) -> Result<usize, Error> {
    let index = usize::try_from(value).map_err(|_| Error::config("invalid argv index"))?;
    if index > ARGUMENTS {
        return Err(Error::config("argv collector index exceeds 255"));
    }
    Ok(index)
}

fn limit(value: u64) -> Result<usize, Error> {
    let limit = usize::try_from(value).map_err(|_| Error::config("invalid collector limit"))?;
    if !(1..=LIMIT).contains(&limit) {
        return Err(Error::config("collector limit must be 1..=4096"));
    }
    Ok(limit)
}

fn selector(value: &str) -> Result<Selector, Error> {
    match value {
        "directory" => Ok(Selector::Directory),
        "executable" => Ok(Selector::Executable),
        "id" => Ok(Selector::Id),
        other => Err(Error::config(format!(
            "unknown process collector selector: {other}"
        ))),
    }
}

fn validate(binding: &str) -> Result<(), Error> {
    if binding.is_empty() || binding.len() > 128 {
        return Err(Error::config(
            "collector binding must contain 1..=128 bytes",
        ));
    }
    if !binding
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"._-:/".contains(&byte))
    {
        return Err(Error::config(format!(
            "invalid collector binding: {binding}"
        )));
    }
    Ok(())
}

fn bounded(source: &str, value: OsString, limit: usize) -> Result<String, Error> {
    let value = value
        .into_string()
        .map_err(|_| Error::collector(format!("{source} collector produced non-UTF-8 text")))?;
    if value.is_empty() || value.len() > limit {
        return Err(Error::collector(format!(
            "{source} collector output must contain 1..={limit} bytes"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(Error::collector(format!(
            "{source} collector output contains control characters"
        )));
    }
    Ok(value)
}

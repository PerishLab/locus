use locus::{Atom, Key, Role};
use serde_json::Value;

#[derive(Clone, Debug)]
pub enum Condition {
    Context(Role, Key),
    Payload(String, String),
}

impl Condition {
    pub fn parse(text: &str) -> Result<Self, String> {
        let invalid =
            || format!("condition {text:?} is not context.ROLE=KEY or payload.FIELD=VALUE");
        let (name, value) = text.split_once('=').ok_or_else(invalid)?;
        if let Some(role) = name.strip_prefix("context.") {
            let role = Role::new(role).map_err(|error| error.to_string())?;
            let key = Key::new(value).map_err(|error| error.to_string())?;
            return Ok(Self::Context(role, key));
        }
        match name.strip_prefix("payload.") {
            Some(field) if !field.is_empty() => Ok(Self::Payload(field.into(), value.into())),
            _ => Err(invalid()),
        }
    }

    pub fn holds(&self, atom: &Atom) -> bool {
        match self {
            Self::Context(role, key) => {
                atom.context().get(role.text()).map(String::as_str) == Some(key.text())
            }
            Self::Payload(field, value) => atom
                .payload()
                .and_then(|payload| payload.get(field))
                .is_some_and(|found| scalar(found, value)),
        }
    }
}

fn scalar(found: &Value, value: &str) -> bool {
    match found {
        Value::String(text) => text == value,
        Value::Number(number) => number.to_string() == value,
        Value::Bool(flag) => flag.to_string() == value,
        _ => false,
    }
}

pub fn all(conditions: &[Condition], atom: &Atom) -> bool {
    !conditions.is_empty() && conditions.iter().all(|condition| condition.holds(atom))
}

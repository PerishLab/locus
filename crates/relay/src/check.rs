use axum::http::StatusCode;
use locus::Atom;
use serde_json::Value;

const LINES: usize = 512;
const FIELDS: &[&str] = &[
    "at",
    "id",
    "producer",
    "context",
    "choices",
    "collections",
    "source",
    "payload",
];
const FORBIDDEN: &[&str] = &[
    "password",
    "pass",
    "token",
    "secret",
    "authorization",
    "cookie",
    "verifier",
    "rescue",
    "sudo",
];

pub(crate) struct Entry {
    pub(crate) id: String,
    pub(crate) line: String,
}

pub(crate) fn batch(raw: &[u8], producer: &str) -> Result<Vec<Entry>, StatusCode> {
    let text = std::str::from_utf8(raw).map_err(|_| StatusCode::BAD_REQUEST)?;
    let trimmed = text.strip_suffix('\n').unwrap_or(text);
    if trimmed.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let parts: Vec<&str> = trimmed.split('\n').collect();
    if parts.len() > LINES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let mut entries = Vec::with_capacity(parts.len());
    for line in parts {
        let id = validate(line, producer)?;
        entries.push(Entry {
            id,
            line: line.to_owned(),
        });
    }
    Ok(entries)
}

fn validate(line: &str, producer: &str) -> Result<String, StatusCode> {
    if line.is_empty() || line.contains('\r') {
        return Err(StatusCode::BAD_REQUEST);
    }
    let value: Value = serde_json::from_str(line).map_err(|_| StatusCode::BAD_REQUEST)?;
    let map = value.as_object().ok_or(StatusCode::BAD_REQUEST)?;
    if map.keys().any(|key| !FIELDS.contains(&key.as_str())) || !inspect(&value) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let atom: Atom = serde_json::from_value(value).map_err(|_| StatusCode::BAD_REQUEST)?;
    let id = atom
        .id()
        .filter(|value| !value.is_empty())
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_owned();
    if atom.producer() != Some(producer) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let tainted = atom
        .choices()
        .iter()
        .any(|item| forbidden(item.role()) || forbidden(item.key()))
        || atom.collections().iter().any(|item| forbidden(item.role()));
    if tainted {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(id)
}

fn inspect(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .all(|(key, item)| !forbidden(key) && inspect(item)),
        Value::Array(items) => items.iter().all(inspect),
        _ => true,
    }
}

pub(crate) fn forbidden(key: &str) -> bool {
    if key.eq_ignore_ascii_case("code_verifier") {
        return true;
    }
    let mut chars = key.chars().peekable();
    let mut prev = None;
    let mut word = String::new();
    while let Some(ch) = chars.next() {
        if matches!(ch, '_' | '-' | '.' | '/') {
            if blocked(&word) {
                return true;
            }
            word.clear();
            prev = None;
            continue;
        }
        let next = chars.peek().copied();
        if boundary(&word, prev, ch, next) {
            if blocked(&word) {
                return true;
            }
            word.clear();
        }
        word.push(ch.to_ascii_lowercase());
        prev = Some(ch);
    }
    blocked(&word)
}

fn boundary(word: &str, prev: Option<char>, ch: char, next: Option<char>) -> bool {
    if word.is_empty() || !ch.is_ascii_uppercase() {
        return false;
    }
    let lower = prev.is_some_and(|p| p.is_ascii_lowercase() || p.is_ascii_digit());
    let acronym = prev.is_some_and(|p| p.is_ascii_uppercase())
        && next.is_some_and(|n| n.is_ascii_lowercase());
    lower || acronym
}

fn blocked(word: &str) -> bool {
    !word.is_empty() && FORBIDDEN.contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive() {
        for bad in [
            "password",
            "userPassword",
            "access_token",
            "auth-token",
            "http.authorization",
            "header/Cookie",
            "APIKeySecret",
            "code_verifier",
            "codeVerifier",
            "myPass",
            "rescueCode",
            "sudoMode",
            "HTTPAuthorization",
        ] {
            assert!(forbidden(bad), "{bad}");
        }
        for good in [
            "compass",
            "bypass",
            "passage",
            "tokenizer",
            "secretary",
            "locus.trace",
            "pagePath",
            "durationMs",
        ] {
            assert!(!forbidden(good), "{good}");
        }
    }
}

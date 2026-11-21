use crate::origin::Origin;
use locus_derivation::quality::{Attempt, BYTES, LIMIT, Quality, Window};
use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(clap::Args)]
pub(super) struct Access {
    #[arg(
        long,
        default_value = "http://127.0.0.1:43308",
        env = "LOCUS_API",
        hide_env_values = true,
        value_name = "URL",
        help = "Locus API to read"
    )]
    api: String,
    #[arg(long, value_name = "INSTANT", value_parser = crate::origin::instant::parse, help = "Receive time at inclusive window start; ns or zoned RFC3339")]
    from: u64,
    #[arg(long, value_name = "INSTANT", value_parser = crate::origin::instant::parse, help = "Receive time at exclusive window end; ns or zoned RFC3339")]
    to: u64,
    #[command(flatten)]
    credentials: crate::origin::Credentials,
}

impl Access {
    pub fn origin(self) -> (Origin, Window) {
        let window = Window {
            from: self.from,
            to: self.to,
        };
        (
            Origin::slice(self.api, window, self.credentials, "locus-adapter-codex"),
            window,
        )
    }
}

const LINE: usize = 16 * 1024;

pub(super) fn diagnostics(
    path: Option<&Path>,
    excluded: &BTreeSet<String>,
    quality: &mut Quality,
    commands: &mut Quality,
) -> Result<u64, String> {
    let Some(path) = path else {
        return Ok(0);
    };
    let mut file = File::open(path)
        .map_err(|_| "cannot open explicit diagnostic log")?
        .take(BYTES as u64 + 1);
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| "cannot read explicit diagnostic log")?;
    if bytes.len() > BYTES {
        return Err("diagnostic input byte limit exceeded".into());
    }
    if !bytes.is_empty() && bytes.last() != Some(&b'\n') {
        return Err("diagnostic input ends with an incomplete record".into());
    }
    let mut legacy = 0;
    for (index, line) in bytes.split_inclusive(|b| *b == b'\n').enumerate() {
        if index >= LIMIT || line.len() > LINE {
            return Err("diagnostic record limit exceeded".into());
        }
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        if old(line) {
            legacy += 1;
            continue;
        }
        let value: serde_json::Value = serde_json::from_slice(line)
            .map_err(|_| format!("invalid structured diagnostic record {}", index + 1))?;
        if value.get("schema").and_then(serde_json::Value::as_str)
            == Some("locus.adapter.attempt/v2")
        {
            let batch: Batch =
                serde_json::from_value(value).map_err(|_| "invalid paired outcome")?;
            batch.validate()?;
            observe(batch.hook, excluded, quality)?;
            observe(batch.command, excluded, commands)?;
        } else {
            let attempt: Attempt =
                serde_json::from_value(value).map_err(|_| "invalid adapter outcome")?;
            observe(attempt, excluded, quality)?;
        }
    }
    Ok(legacy)
}

fn old(line: &[u8]) -> bool {
    line.strip_prefix(b"locus adapter diagnostic [")
        .and_then(|text| text.strip_suffix(b"]"))
        .is_some_and(|code| {
            !code.is_empty()
                && code.len() <= 64
                && code
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(b))
        })
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    schema: String,
    hook: Attempt,
    command: Attempt,
}

fn observe(
    attempt: Attempt,
    excluded: &BTreeSet<String>,
    quality: &mut Quality,
) -> Result<(), String> {
    attempt.validate()?;
    if attempt
        .group
        .as_ref()
        .is_none_or(|group| !excluded.contains(group))
    {
        quality.attempt(attempt)?;
    }
    Ok(())
}

impl Batch {
    fn validate(&self) -> Result<(), String> {
        if self.schema != "locus.adapter.attempt/v2"
            || self.hook.event.as_deref() != Some("PreToolUse")
            || self.command.event.as_deref() != Some("CommandShape")
        {
            return Err("invalid paired event contract".into());
        }
        if self.hook.group.is_none()
            || self.hook.group != self.command.group
            || self
                .hook
                .id
                .as_ref()
                .zip(self.command.id.as_ref())
                .is_some_and(|(hook, command)| hook == command)
        {
            return Err("inconsistent paired identity".into());
        }
        if self
            .hook
            .atom
            .as_ref()
            .zip(self.command.atom.as_ref())
            .is_some_and(|(hook, command)| hook == command)
        {
            return Err("paired outcomes reuse an Atom identity".into());
        }
        Ok(())
    }
}

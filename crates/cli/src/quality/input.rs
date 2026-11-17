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
        value_name = "URL",
        help = "Locus API to read"
    )]
    api: String,
    #[arg(long, value_name = "INSTANT", value_parser = crate::origin::instant::parse, help = "Receive time at inclusive window start; ns or zoned RFC3339")]
    from: u64,
    #[arg(long, value_name = "INSTANT", value_parser = crate::origin::instant::parse, help = "Receive time at exclusive window end; ns or zoned RFC3339")]
    to: u64,
    #[arg(
        long,
        env = "LOCUS_API_TOKEN",
        hide_env_values = true,
        help = "Reader bearer token; never observed"
    )]
    token: Option<String>,
}

impl Access {
    pub fn origin(self) -> (Origin, Window) {
        let window = Window {
            from: self.from,
            to: self.to,
        };
        (
            Origin::slice(self.api, window, self.token, "locus-adapter-codex"),
            window,
        )
    }
}

const LINE: usize = 16 * 1024;

pub(super) fn diagnostics(
    path: Option<&Path>,
    excluded: &BTreeSet<String>,
    quality: &mut Quality,
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
        let attempt: Attempt = serde_json::from_slice(line)
            .map_err(|_| format!("invalid structured diagnostic record {}", index + 1))?;
        attempt.validate()?;
        if attempt
            .group
            .as_ref()
            .is_none_or(|group| !excluded.contains(group))
        {
            quality.attempt(attempt)?;
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

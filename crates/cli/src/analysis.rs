mod capture;
mod derive;

use crate::origin::Origin;
use clap::Args;
use locus_derivation::evidence;
use locus_derivation::inspect::Config;
use locus_derivation::span;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

#[derive(Args, Serialize)]
pub struct Limits {
    #[arg(long = "max-bytes", default_value_t = 536_870_912, value_parser = clap::value_parser!(u64).range(1..=536_870_912), help = "Maximum API bytes to capture in memory (hard ceiling 512 MiB)")]
    bytes: u64,
    #[arg(long = "max-records", default_value_t = 1_000_000, value_parser = clap::value_parser!(u64).range(1..=1_000_000), help = "Maximum input records (hard ceiling 1000000; each line at most 1 MiB)")]
    records: u64,
}

#[locus::trace(with = plumb::observation::view())]
pub fn execute(root: &Path, origin: &Origin, limits: Limits) -> ExitCode {
    match run(root, origin, &limits) {
        Ok(clean) if clean => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(1),
        Err(error) => {
            eprintln!("locus: {error}");
            ExitCode::from(2)
        }
    }
}

#[locus::trace(with = plumb::observation::view())]
fn run(root: &Path, origin: &Origin, limits: &Limits) -> Result<bool, String> {
    let config = crate::read(root)?;
    if config.analyzers().len() > 64 {
        return Err("analysis allows at most 64 effective analyzers".into());
    }
    let input = origin.open()?;
    let bytes = capture::read(input.reader, limits)?;
    let (inspection, records) = derive::inspection(&config, &bytes, input.retained)?;
    let span = derive::spans(&bytes, input.retained)?;
    let evidence = derive::evidence(&inspection, &bytes)?;
    let metadata = summary(origin, limits, &config, (&bytes, records, input.retained));
    let mut output = io::stdout().lock();
    for record in inspection.records() {
        emit(&mut output, &record)?;
    }
    let bounds = origin.bounds(input.retained);
    for record in span.records() {
        let encoded = serde_json::to_string(&record).map_err(|error| error.to_string())?;
        let encoded = match record {
            span::Record::Summary(_) => bounds.mark(encoded),
            _ => encoded,
        };
        writeln!(output, "{encoded}").map_err(|error| error.to_string())?;
    }
    for group in evidence.finish() {
        emit(&mut output, &group)?;
    }
    emit(&mut output, &metadata)?;
    Ok(inspection.clean())
}

fn summary(
    origin: &Origin,
    limits: &Limits,
    config: &Config,
    read: (&[u8], u64, Option<u64>),
) -> Value {
    let (bytes, records, retained) = read;
    json!({
        "schema": "locus.analyze/v1",
        "kind": "summary",
        "version": plumb::version!("LOCUS"),
        "request": origin,
        "records": records,
        "retained": retained,
        "bytes": bytes.len(),
        "sha256": format!("{:x}", Sha256::digest(bytes)),
        "config": config,
        "limits": {
            "input": limits,
            "line_bytes": capture::LINE,
            "analyzers": 64,
            "evidence_groups": evidence::GROUPS,
            "references_per_group": evidence::REFERENCES,
            "contributions_per_group": evidence::CONTRIBUTIONS,
            "evidence_content_bytes": evidence::BYTES,
        },
    })
}

fn emit(output: &mut impl Write, record: &impl Serialize) -> Result<(), String> {
    serde_json::to_writer(&mut *output, record).map_err(|error| error.to_string())?;
    writeln!(output).map_err(|error| format!("cannot write analysis: {error}"))
}

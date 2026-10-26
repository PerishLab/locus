use crate::origin::Origin;
use clap::Args;
use locus_derivation::tree::{Condition, Record, Tree};
use serde::Serialize;
use serde_json::{Value, json};
use std::io::{self, Write};
use std::process::ExitCode;

#[derive(Args)]
pub struct Selection {
    #[arg(
        long = "seed",
        value_name = "CONDITION",
        required = true,
        help = "Select seed Atoms: context.ROLE=KEY or payload.FIELD=VALUE; repeated conditions all hold"
    )]
    seeds: Vec<String>,
    #[arg(
        long = "show",
        value_name = "CONDITION",
        help = "Also emit tree Atoms matching every such condition, in the seed grammar"
    )]
    show: Vec<String>,
    #[command(flatten)]
    limits: Limits,
}

#[derive(Args, Serialize)]
pub struct Limits {
    #[arg(long = "max-trees", default_value_t = 1_000, value_parser = clap::value_parser!(u64).range(1..), help = "Refuse when seeds reach more trees")]
    trees: u64,
    #[arg(long = "max-atoms", default_value_t = 100_000, value_parser = clap::value_parser!(u64).range(1..), help = "Refuse when more seed or shown Atoms must be held")]
    atoms: u64,
    #[arg(long = "max-keys", default_value_t = 2_000_000, value_parser = clap::value_parser!(u64).range(1..), help = "Refuse when more trace and span keys must be tracked")]
    keys: u64,
}

#[locus::trace(with = plumb::observation::view())]
pub fn execute(selection: &Selection, origin: &Origin) -> ExitCode {
    match run(selection, origin) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("locus: {error}");
            ExitCode::from(2)
        }
    }
}

fn parse(conditions: &[String]) -> Result<Vec<Condition>, String> {
    conditions
        .iter()
        .map(|text| Condition::parse(text))
        .collect()
}

#[locus::trace(with = plumb::observation::view())]
fn run(selection: &Selection, origin: &Origin) -> Result<(), String> {
    let limits = &selection.limits;
    let mut tree = Tree::new(parse(&selection.seeds)?, parse(&selection.show)?)?
        .trees(limits.trees)
        .atoms(limits.atoms)
        .keys(limits.keys);
    let input = origin.open()?;
    let mut bytes = 0_u64;
    crate::input::scan(input.reader, |atom, encoded| {
        bytes = bytes.saturating_add(encoded);
        tree.observe(atom)
    })?;
    let report = tree.finish()?.retained(input.retained);
    let bounds = origin.bounds(input.retained);
    let mut output = io::stdout().lock();
    for record in report.records() {
        let encoded = match &record {
            Record::Summary(_) => {
                let mut value = serde_json::to_value(&record).map_err(|error| error.to_string())?;
                extend(&mut value, origin, selection, bytes);
                bounds.mark(value.to_string())
            }
            _ => serde_json::to_string(&record).map_err(|error| error.to_string())?,
        };
        writeln!(output, "{encoded}").map_err(|error| format!("cannot write tree: {error}"))?;
    }
    Ok(())
}

fn extend(value: &mut Value, origin: &Origin, selection: &Selection, bytes: u64) {
    if let Some(summary) = value.as_object_mut() {
        summary.insert("version".into(), json!(plumb::version!("LOCUS")));
        summary.insert("request".into(), json!(origin));
        summary.insert("seed".into(), json!(selection.seeds));
        summary.insert("show".into(), json!(selection.show));
        summary.insert("bytes".into(), json!(bytes));
        summary.insert("limits".into(), json!(selection.limits));
    }
}

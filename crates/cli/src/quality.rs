mod input;
mod project;

use clap::{Args, Parser, Subcommand};
use locus_derivation::quality::{BYTES, Fact, LIMIT, Quality, Report, key};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Serialize)]
struct Output {
    #[serde(flatten)]
    report: Report,
    source: Selection,
    commands: Report,
}

#[derive(Serialize)]
struct Selection {
    producer: &'static str,
    expected: Option<BTreeSet<String>>,
    excluded: BTreeSet<String>,
    diagnostics: bool,
}

#[derive(Parser)]
#[command(name = "locus quality")]
struct Cli {
    #[command(subcommand)]
    source: Source,
}

#[derive(Subcommand)]
enum Source {
    #[command(
        about = "Reconcile Codex adapter outcomes with one bounded API read",
        after_help = "Exit 0 means a report was emitted, not complete capture or healthy ingestion. Requires an explicit [from,to) receive-time window. Exact producer locus-adapter-codex. Root counts describe hook records and calls; commands separately reconciles command.v1 Atoms, coverage and outcomes. Both v1 and paired v2 diagnostics are supported. Reads at most 16 MiB per API/log input and 10000 facts/identities; diagnostic lines are at most 16 KiB. --expect-session is caller-declared scope, not discovery or a proven active population. Missing diagnostics or expected scope remain unknown. Legacy bare diagnostics have no known window. Buffer handoff is not central persistence; unmatched IDs may reflect delay, retention or window boundaries. No business success, exact process duration or source emitter version is derived. Adapter failures stay outside the Atom reporter. An absent final report means refused/incomplete output."
    )]
    Codex(Options),
}

#[derive(Args)]
struct Options {
    #[command(flatten)]
    access: input::Access,
    #[arg(
        long,
        help = "Explicit private structured adapter stderr log; omitted means unknown losses"
    )]
    diagnostics: Option<PathBuf>,
    #[arg(
        long = "expect-session",
        help = "Caller-declared expected session; repeatable"
    )]
    expected: Vec<String>,
    #[arg(
        long = "exclude-session",
        help = "Exclude a session consistently from API and diagnostic evidence; repeatable"
    )]
    excluded: Vec<String>,
}

pub(crate) fn run(args: Vec<String>) -> ExitCode {
    let cli = match Cli::try_parse_from(std::iter::once("locus quality".into()).chain(args)) {
        Ok(cli) => cli,
        Err(error) => {
            let code = if error.use_stderr() { 2 } else { 0 };
            let _ = error.print();
            return ExitCode::from(code);
        }
    };
    let Source::Codex(options) = cli.source;
    match execute(options).and_then(emit) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("locus quality: {error}");
            ExitCode::from(2)
        }
    }
}

fn execute(options: Options) -> Result<Output, String> {
    let (origin, window) = options.access.origin();
    let excluded = identities(options.excluded)?;
    let supplied = !options.expected.is_empty();
    let expected = identities(options.expected)?
        .difference(&excluded)
        .cloned()
        .collect();
    let expected = supplied.then_some(expected);
    let mut quality = Quality::new(window, expected.clone())?;
    let mut commands = Quality::new(window, expected.clone())?;
    let legacy = input::diagnostics(
        options.diagnostics.as_deref(),
        &excluded,
        &mut quality,
        &mut commands,
    )?;
    let opened = origin.open()?;
    let mut reader = opened.reader.take(BYTES as u64 + 1);
    let mut records = 0;
    let mut identities = BTreeMap::new();
    crate::input::scan(&mut reader, |atom, _| {
        records += 1;
        if records > LIMIT {
            return Err("API input record limit exceeded".into());
        }
        let fact = project::fact(atom)?;
        if !excluded.contains(&fact.group) {
            observe(fact, &mut quality, &mut commands, &mut identities)?;
        }
        Ok(())
    })
    .map_err(|_| "API input is invalid, interrupted or beyond quality limits")?;
    if reader.limit() == 0 {
        return Err("API input byte limit exceeded".into());
    }
    let diagnostics = options.diagnostics.is_some();
    Ok(Output {
        report: quality.finish(diagnostics, legacy, opened.retained),
        commands: commands.finish(diagnostics, 0, opened.retained),
        source: Selection {
            producer: "locus-adapter-codex",
            expected,
            excluded,
            diagnostics,
        },
    })
}

fn identities(values: Vec<String>) -> Result<BTreeSet<String>, String> {
    if values.len() > LIMIT || values.iter().any(|value| !key(value)) {
        return Err("invalid or excessive explicit session scope".into());
    }
    Ok(values.into_iter().collect())
}

fn emit(report: Output) -> Result<(), String> {
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, &report).map_err(|_| "cannot encode quality report")?;
    writeln!(output).map_err(|_| "cannot write quality report".into())
}

fn observe(
    fact: Fact,
    hooks: &mut Quality,
    commands: &mut Quality,
    identities: &mut BTreeMap<String, bool>,
) -> Result<(), String> {
    let command = fact.event == "CommandShape";
    if identities
        .get(&fact.id)
        .is_some_and(|previous| *previous != command)
    {
        return Err("Atom identity crosses projection kinds".into());
    }
    identities.insert(fact.id.clone(), command);
    if command {
        commands.observe(fact)
    } else {
        hooks.observe(fact)
    }
}

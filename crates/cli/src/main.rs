mod derive;
mod input;
mod inspect;
mod origin;
mod query;
mod span;

use clap::{Parser, Subcommand};
use origin::Origin;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "locus", version = plumb::version!("LOCUS"))]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Inspect {
        #[arg(default_value = ".")]
        root: PathBuf,
        #[command(flatten)]
        origin: Origin,
    },
    Query {
        role: String,
        key: Option<String>,
        #[command(flatten)]
        origin: Origin,
    },
    Span {
        #[command(flatten)]
        origin: Origin,
    },
}

fn main() -> ExitCode {
    if let Err(error) = plumb::identity!("LOCUS") {
        eprintln!("locus: {error}");
        return ExitCode::from(1);
    }
    match Cli::parse().command {
        Command::Inspect { root, origin } => inspect(root, &origin),
        Command::Query { role, key, origin } => query(role, key, &origin),
        Command::Span { origin } => span(&origin),
    }
}

fn inspect(root: PathBuf, origin: &Origin) -> ExitCode {
    let config = match inspect::Config::read(&root) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("locus: {error}");
            return ExitCode::from(2);
        }
    };
    let input = match origin.open() {
        Ok(input) => input,
        Err(error) => {
            eprintln!("locus: {error}");
            return ExitCode::from(2);
        }
    };
    let report =
        match inspect::scan(input.reader, &config).map(|report| report.retained(input.retained)) {
            Ok(report) => report,
            Err(error) => {
                eprintln!("locus: {error}");
                return ExitCode::from(2);
            }
        };
    let output = io::stdout();
    let mut output = output.lock();
    for record in report.records() {
        if let Err(error) = serde_json::to_writer(&mut output, &record)
            .and_then(|_| writeln!(output).map_err(serde_json::Error::io))
        {
            eprintln!("locus: cannot write inspection: {error}");
            return ExitCode::from(2);
        }
    }
    if report.clean() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn query(role: String, key: Option<String>, origin: &Origin) -> ExitCode {
    let selector = match query::Selector::new(role, key) {
        Ok(selector) => selector,
        Err(error) => {
            eprintln!("locus: {error}");
            return ExitCode::from(2);
        }
    };
    let input = match origin.open() {
        Ok(input) => input,
        Err(error) => {
            eprintln!("locus: {error}");
            return ExitCode::from(2);
        }
    };
    let report =
        match query::scan(input.reader, selector).map(|report| report.retained(input.retained)) {
            Ok(report) => report,
            Err(error) => {
                eprintln!("locus: {error}");
                return ExitCode::from(2);
            }
        };
    let output = io::stdout();
    let mut output = output.lock();
    for record in report.records() {
        if let Err(error) = serde_json::to_writer(&mut output, &record)
            .and_then(|_| writeln!(output).map_err(serde_json::Error::io))
        {
            eprintln!("locus: cannot write query: {error}");
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

fn span(origin: &Origin) -> ExitCode {
    let input = match origin.open() {
        Ok(input) => input,
        Err(error) => {
            eprintln!("locus: {error}");
            return ExitCode::from(2);
        }
    };
    let report = match span::scan(input.reader).map(|report| report.retained(input.retained)) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("locus: {error}");
            return ExitCode::from(2);
        }
    };
    let output = io::stdout();
    let mut output = output.lock();
    for record in report.records() {
        if let Err(error) = serde_json::to_writer(&mut output, &record)
            .and_then(|_| writeln!(output).map_err(serde_json::Error::io))
        {
            eprintln!("locus: cannot write derivation: {error}");
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

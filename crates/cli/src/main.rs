mod input;
mod inspect;
mod query;

use clap::{Parser, Subcommand};
use std::io::{self, BufReader, Write};
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
    },
    Query {
        role: String,
        key: Option<String>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Inspect { root } => inspect(root),
        Command::Query { role, key } => query(role, key),
    }
}

fn inspect(root: PathBuf) -> ExitCode {
    let config = match inspect::Config::read(&root) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("locus: {error}");
            return ExitCode::from(2);
        }
    };
    let input = io::stdin();
    let report = match inspect::scan(BufReader::new(input.lock()), &config) {
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

fn query(role: String, key: Option<String>) -> ExitCode {
    let selector = match query::Selector::new(role, key) {
        Ok(selector) => selector,
        Err(error) => {
            eprintln!("locus: {error}");
            return ExitCode::from(2);
        }
    };
    let input = io::stdin();
    let report = match query::scan(BufReader::new(input.lock()), selector) {
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

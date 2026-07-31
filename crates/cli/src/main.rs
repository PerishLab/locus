mod inspect;

use clap::{Parser, Subcommand};
use std::io::{self, BufReader, Write};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "locus", version = plumb::version!("LOCUS"))]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Inspect,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Inspect => run(),
    }
}

fn run() -> ExitCode {
    let input = io::stdin();
    let findings = match inspect::scan(BufReader::new(input.lock())) {
        Ok(findings) => findings,
        Err(error) => {
            eprintln!("locus: {error}");
            return ExitCode::from(2);
        }
    };
    let output = io::stdout();
    let mut output = output.lock();
    for finding in &findings {
        if let Err(error) = serde_json::to_writer(&mut output, finding)
            .and_then(|_| writeln!(output).map_err(serde_json::Error::io))
        {
            eprintln!("locus: cannot write finding: {error}");
            return ExitCode::from(2);
        }
    }
    if findings.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

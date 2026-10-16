mod input;
mod observation;
mod origin;

use clap::{Parser, Subcommand};
use locus_derivation::inspect::{Config, Inspection};
use locus_derivation::query::{Query, Selector};
use locus_derivation::span::Span;
use origin::Origin;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "locus", version = plumb::version!("LOCUS"), after_help = "CLI observation is disabled by default. Set [locus] enabled = true in config.toml under ~/.locus on Unix or LOCALAPPDATA/locus on Windows. Buffered observations use state/locus there. endpoint in [locus] overrides LOCUS_API, then http://127.0.0.1:43308. The business --api option only chooses the read API; it does not select the observation endpoint. Observation failures preserve reports and exit codes.")]
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
    let command = Cli::parse().command;
    let observer = observation::start(name(&command));
    let code = execute(command);
    if let Some(observer) = observer {
        let number = if code == ExitCode::SUCCESS {
            0
        } else if code == ExitCode::from(1) {
            1
        } else {
            2
        };
        observer.finish(number);
    }
    code
}

fn name(command: &Command) -> &'static str {
    match command {
        Command::Inspect { .. } => "inspect",
        Command::Query { .. } => "query",
        Command::Span { .. } => "span",
    }
}

#[locus::trace(with = plumb::observation::view())]
fn execute(command: Command) -> ExitCode {
    match command {
        Command::Inspect { root, origin } => inspect(root, &origin),
        Command::Query { role, key, origin } => query(role, key, &origin),
        Command::Span { origin } => span(&origin),
    }
}

#[locus::trace(with = plumb::observation::view())]
fn inspect(root: PathBuf, origin: &Origin) -> ExitCode {
    let config = match read(&root) {
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
    let mut inspection = Inspection::new(&config);
    let report = match input::scan(input.reader, |atom, encoded| {
        inspection.observe(&atom, encoded)
    }) {
        Ok(_) => inspection.finish().retained(input.retained),
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

#[locus::trace(with = plumb::observation::view())]
fn query(role: String, key: Option<String>, origin: &Origin) -> ExitCode {
    let selector = match Selector::new(role, key) {
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
    let mut query = Query::new(selector);
    let report = match input::scan(input.reader, |atom, _| query.observe(atom)) {
        Ok(_) => query.finish().retained(input.retained),
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

#[locus::trace(with = plumb::observation::view())]
fn span(origin: &Origin) -> ExitCode {
    let input = match origin.open() {
        Ok(input) => input,
        Err(error) => {
            eprintln!("locus: {error}");
            return ExitCode::from(2);
        }
    };
    let mut span = Span::default();
    let report = match input::scan(input.reader, |atom, _| span.observe(&atom)) {
        Ok(_) => span.finish().retained(input.retained),
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

fn read(root: &Path) -> Result<Config, String> {
    if !root.is_dir() {
        return Err(format!("cannot read {}: not a directory", root.display()));
    }
    let path = root.join("locus.toml");
    match fs::read_to_string(&path) {
        Ok(content) => Config::parse(&path.display().to_string(), &content),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Config::default()),
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

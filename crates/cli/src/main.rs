mod analysis;
mod incomplete;
mod input;
mod instant;
mod observation;
mod origin;
mod signal;
mod tree;

use clap::{Parser, Subcommand};
use locus_derivation::inspect::{Config, Inspection};
use locus_derivation::query::{Query, Selector};
use locus_derivation::span::{Record, Span};
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
    #[command(
        about = "Inspect one API read with span statistics and bounded finding evidence",
        after_help = "Emits inspect/span records, evidence groups and a final locus.analyze/v1 summary. A missing final summary means incomplete output. Exit 0: clean; 1: findings; 2: refused. Captured input is bounded, not an ingestion watermark."
    )]
    Analyze {
        #[arg(default_value = ".")]
        root: PathBuf,
        #[command(flatten)]
        origin: Origin,
        #[command(flatten)]
        limits: analysis::Limits,
    },
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
    #[command(
        after_help = "The summary adds incomplete only when a read boundary can explain its headless, unclosed or missing-link counts: window.from, window.to, retention, or spool when the window reaches the last 75 seconds of undrained spool data. It names possible boundaries, not a cause; widen the window or wait and reread before treating those counts as defects."
    )]
    Span {
        #[command(flatten)]
        origin: Origin,
    },
    #[command(
        about = "Expand seed Atoms to their linked trace trees",
        after_help = "A tree joins traces only through same-trace containment and resolved locus.parent links: a parent resolves to the one other trace binding that span. Unobserved, ambiguous or conflicting parents leave a trace unlinked and are counted; a detached root carries such a parent. Emits one tree record per tree followed by its seed and shown Atoms verbatim, then a final locus.tree/v1 summary. A missing final summary means incomplete output. --select and --producer narrow the read itself, so trees see only that slice. Trees show what the read window holds; they claim no lifecycle, success, cause or invocation identity. Exit 0: emitted; 2: refused without partial output."
    )]
    Tree {
        #[command(flatten)]
        selection: tree::Selection,
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
    signal::watch();
    let observer = observation::start(name(&command));
    let code = execute(command);
    let number = signal::received().unwrap_or(if code == ExitCode::SUCCESS {
        0
    } else if code == ExitCode::from(1) {
        1
    } else {
        2
    });
    if let Some(observer) = observer {
        observer.finish(number.into());
    }
    ExitCode::from(number)
}

fn name(command: &Command) -> &'static str {
    match command {
        Command::Analyze { .. } => "analyze",
        Command::Inspect { .. } => "inspect",
        Command::Query { .. } => "query",
        Command::Span { .. } => "span",
        Command::Tree { .. } => "tree",
    }
}

#[locus::trace(with = plumb::observation::view())]
fn execute(command: Command) -> ExitCode {
    match command {
        Command::Analyze {
            root,
            origin,
            limits,
        } => analysis::execute(&root, &origin, limits),
        Command::Inspect { root, origin } => inspect(root, &origin),
        Command::Query { role, key, origin } => query(role, key, &origin),
        Command::Span { origin } => span(&origin),
        Command::Tree { selection, origin } => tree::execute(&selection, &origin),
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
    let bounds = origin.bounds(input.retained);
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
        let written = serde_json::to_string(&record).and_then(|encoded| {
            let line = match record {
                Record::Summary(_) => bounds.mark(encoded),
                _ => encoded,
            };
            writeln!(output, "{line}").map_err(serde_json::Error::io)
        });
        if let Err(error) = written {
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

mod codex;
mod record;
mod tool;

use clap::{Parser, Subcommand};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const LIMIT: u64 = 1024 * 1024;

#[derive(Parser)]
#[command(
    name = "locus adapt",
    about = "Adapt external observations through the Locus engine"
)]
struct Cli {
    #[command(subcommand)]
    source: Source,
}

#[derive(Subcommand)]
enum Source {
    #[command(
        about = "Accept one Codex 0.169 hook JSON object from stdin",
        after_help = "Explicit invocation enables collection. Input is limited to 1 MiB. Writes a minimal whitelist through the native api reporter as locus-adapter-codex. Emits {} and exits 0 even on collection failure; diagnostics go to stderr, never hook decisions or model context. Invalid adapter arguments also use this hook-safe behavior. No prompt, command, response body, cwd or transcript path is retained. Unknown events refuse. Atom time is acceptance time, not the original event time. Pre/PostToolUse describe hook observations, not process lifetime or business completion. Keep the command synchronous and use a private buffer shared by this adapter only; async hooks can reorder or be cancelled. Global hook installation is separate."
    )]
    Codex {
        #[arg(
            long,
            help = "Private native reporter buffer directory; required opt-in"
        )]
        buffer: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:43308")]
        endpoint: String,
    },
}

pub(crate) fn run(args: Vec<String>) -> ExitCode {
    let cli = Cli::try_parse_from(std::iter::once("locus adapt".into()).chain(args));
    let cli = match cli {
        Ok(cli) => cli,
        Err(error) if error.use_stderr() => return finish(Err("arguments.invalid")),
        Err(error) => {
            let _ = write!(io::stdout().lock(), "{error}");
            return ExitCode::SUCCESS;
        }
    };
    finish(collect(cli.source))
}

fn finish(result: Result<(), &'static str>) -> ExitCode {
    if let Err(code) = result {
        diagnose(code);
    }
    let _ = writeln!(io::stdout().lock(), "{{}}");
    ExitCode::SUCCESS
}

fn diagnose(code: &str) {
    let _ = writeln!(io::stderr().lock(), "locus adapter diagnostic [{code}]");
}

fn collect(source: Source) -> Result<(), &'static str> {
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "input.read")?;
    if bytes.len() as u64 > LIMIT {
        return Err("input.limit");
    }
    let event = codex::normalize(&bytes)?;
    match source {
        Source::Codex { buffer, endpoint } => record::append(event, buffer, endpoint),
    }
}

mod codex;
mod command;
mod outcome;
mod record;
mod tool;

use clap::{Parser, Subcommand};
use locus_derivation::quality::{Attempt, Progress, Stage};
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
        after_help = "Explicit invocation enables collection. Input is limited to 1 MiB. Writes a minimal whitelist as locus-adapter-codex. With --key-file and an HTTPS origin, accepted records enter a dedicated durable queue and are sent directly. Use locus deliver to retry. With a numeric HTTP endpoint, the adapter registers the legacy api spool. Emits {} and exits 0 even on collection failure; one bounded outcome goes to stderr per completed invocation (v1 hook-only, v2 paired hook/command outcomes), never hook decisions or model context. Its local attempt identity is not a source-event ID. Accepted means Engine acceptance; buffer written is not API persistence. Missing invocation, process loss or stderr write failure remain unknown. Invalid adapter arguments also use this hook-safe behavior. No prompt, command, response body, cwd or transcript path is retained. Unknown events refuse. Bash PreToolUse also projects one source-less bounded command tuple, scoped by codex.record=command.v1. Select codex.command=known.v1 before prefix inspection; unknown coverage is not known command repetition. Atom time is acceptance time, not the original event time. Pre/PostToolUse describe hook observations, not process lifetime or business completion. Keep the command synchronous and use a private buffer shared by this adapter only; async hooks can reorder or be cancelled. Global hook installation is separate."
    )]
    Codex {
        #[arg(
            long,
            help = "Private native reporter buffer directory; required opt-in"
        )]
        buffer: PathBuf,
        #[arg(long, default_value = "http://127.0.0.1:43308")]
        endpoint: String,
        #[arg(
            long = "key-file",
            value_name = "PATH",
            help = "Use the durable HTTPS reporter with this machine-key file"
        )]
        credential: Option<PathBuf>,
    },
}

pub(crate) fn run(args: Vec<String>) -> ExitCode {
    let mut attempt = outcome::start();
    let cli = Cli::try_parse_from(std::iter::once("locus adapt".into()).chain(args));
    let cli = match cli {
        Ok(cli) => cli,
        Err(error) if error.use_stderr() => {
            attempt
                .issues
                .push(outcome::Fault::from("arguments.invalid").issue(Stage::Input));
            return finish(attempt, None);
        }
        Err(error) => {
            let _ = write!(io::stdout().lock(), "{error}");
            return ExitCode::SUCCESS;
        }
    };
    let command = collect(cli.source, &mut attempt);
    finish(attempt, command)
}

fn finish(attempt: Attempt, command: Option<Attempt>) -> ExitCode {
    outcome::emit(&attempt, command.as_ref());
    let _ = writeln!(io::stdout().lock(), "{{}}");
    ExitCode::SUCCESS
}

fn collect(source: Source, attempt: &mut Attempt) -> Option<Attempt> {
    let event = match read(attempt) {
        Ok(event) => event,
        Err(issue) => {
            attempt.outcome = Progress::Rejected {
                stage: issue.stage.clone(),
            };
            attempt.issues.push(issue);
            return None;
        }
    };
    match source {
        Source::Codex {
            buffer,
            endpoint,
            credential,
        } => {
            let (hook, command) = event;
            let target = record::Target {
                buffer,
                endpoint,
                credential,
            };
            let session = record::Session::new(&target);
            session.append(hook, attempt);
            command.map(|event| {
                let mut command = outcome::start();
                command.event = Some("CommandShape".into());
                command.group = Some(event.session.clone());
                session.append(event, &mut command);
                command
            })
        }
    }
}

fn read(
    attempt: &mut Attempt,
) -> Result<(codex::Event, Option<codex::Event>), locus_derivation::quality::Issue> {
    let mut bytes = Vec::new();
    io::stdin()
        .lock()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| outcome::Fault::from("input.read").issue(Stage::Input))?;
    if bytes.len() as u64 > LIMIT {
        return Err(outcome::Fault::from("input.limit").issue(Stage::Input));
    }
    outcome::identify(attempt, &bytes);
    let hook = codex::normalize(&bytes).map_err(|fault| {
        let stage = if fault.code == "input.json" {
            Stage::Input
        } else {
            Stage::Normalize
        };
        fault.issue(stage)
    })?;
    let input = serde_json::from_slice(&bytes)
        .map_err(|_| outcome::Fault::from("input.json").issue(Stage::Input))?;
    let command = command::project(&input, &hook);
    Ok((hook, command))
}

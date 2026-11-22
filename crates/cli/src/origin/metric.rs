use crate::origin::{Credentials, instant};
use locus_derivation::metric::Report;
use std::io::{self, Write};
use std::process::ExitCode;

#[derive(clap::Args)]
pub(crate) struct Args {
    #[arg(
        long,
        env = "LOCUS_API",
        hide_env_values = true,
        default_value = "http://127.0.0.1:43308"
    )]
    api: String,
    #[command(flatten)]
    credentials: Credentials,
    #[arg(long)]
    producer: String,
    #[arg(long)]
    name: String,
    #[arg(long)]
    unit: String,
    #[arg(long, value_parser = ["gauge", "counter"])]
    kind: String,
    #[arg(long, value_parser = instant::parse)]
    from: u64,
    #[arg(long, value_parser = instant::parse)]
    to: u64,
    #[arg(long, help = "Bucket duration in nanoseconds")]
    step: u64,
    #[arg(long, help = "Group by this declared metric label")]
    group: Option<String>,
}

pub(crate) fn execute(args: Args) -> ExitCode {
    match read(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("locus: {error}");
            ExitCode::from(2)
        }
    }
}

fn read(args: Args) -> Result<(), String> {
    let mut api = crate::transport::reader(&args.api)?;
    let token = args.credentials.resolve(&api)?;
    api.path_segments_mut()
        .map_err(|_| "invalid API URL")?
        .pop_if_empty()
        .extend(["api", "v1", "metrics"]);
    let mut request = crate::transport::agent()
        .get(api.as_str())
        .query("producer", args.producer)
        .query("name", args.name)
        .query("kind", args.kind)
        .query("unit", args.unit)
        .query("from", args.from.to_string())
        .query("to", args.to.to_string())
        .query("step", args.step.to_string());
    if let Some(group) = args.group {
        request = request.query("group", group);
    }
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = request.call().map_err(|_| "cannot reach API")?;
    if response.status() != 200 {
        return Err(format!("api refused with {}", response.status()));
    }
    let body = response.into_body().into_reader();
    let report: Report = serde_json::from_reader(body).map_err(|_| "invalid metric report")?;
    let mut output = io::stdout().lock();
    serde_json::to_writer(&mut output, &report).map_err(|e| e.to_string())?;
    writeln!(output).map_err(|e| e.to_string())
}

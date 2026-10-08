use clap::{Args, Parser, Subcommand};
use locus_api::server;
use locus_api::store::{Segments, Store};
use locus_api::takeover::{Source, Takeover};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "locus-api", version = plumb::version!("LOCUS"))]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Record product Atoms faithfully and serve them back")]
    Serve(Serve),
    #[command(name = "export-openapi", about = "Print the OpenAPI document")]
    Openapi,
}

#[derive(Args)]
struct Serve {
    #[arg(long, help = "Directory holding the store and rejected records")]
    home: PathBuf,
    #[arg(
        long,
        default_value = "127.0.0.1:43308",
        help = "Loopback address to serve on"
    )]
    listen: SocketAddr,
    #[arg(
        long = "source",
        value_name = "PRODUCER=PATH",
        value_parser = Source::new,
        help = "Take over a product report file: PRODUCER=PATH (repeatable)"
    )]
    sources: Vec<Source>,
    #[arg(
        long = "spool",
        value_name = "PRODUCER=DIRECTORY",
        value_parser = Source::new,
        help = "Drain a product's Locus spool directory: PRODUCER=DIRECTORY (repeatable)"
    )]
    spools: Vec<Source>,
    #[arg(
        long,
        value_name = "SECONDS",
        default_value_t = 600,
        help = "Seconds a taken file must stay idle before it is removed"
    )]
    grace: u64,
    #[arg(
        long,
        value_name = "MILLISECONDS",
        default_value_t = 1000,
        help = "Milliseconds between takeover cycles"
    )]
    interval: u64,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Serve(serve) => match run(serve) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("locus-api: {error}");
                ExitCode::from(2)
            }
        },
        Command::Openapi => {
            println!("{}", server::document());
            ExitCode::SUCCESS
        }
    }
}

fn run(serve: Serve) -> Result<(), String> {
    if !serve.listen.ip().is_loopback() {
        return Err(format!("{} is not a loopback address", serve.listen));
    }
    let store: Arc<dyn Store> = Arc::new(Segments::open(serve.home.join("store"))?);
    let takeover = Takeover::new(store.clone(), serve.home.join("rejected"))
        .files(serve.sources, Duration::from_secs(serve.grace))
        .spools(serve.spools);
    let interval = Duration::from_millis(serve.interval);
    thread::spawn(move || {
        loop {
            if let Err(error) = takeover.cycle() {
                eprintln!("locus-api: takeover: {error}");
            }
            thread::sleep(interval);
        }
    });
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(serve.listen)
            .await
            .map_err(|error| format!("cannot listen on {}: {error}", serve.listen))?;
        let shutdown = async {
            let _ = tokio::signal::ctrl_c().await;
        };
        server::serve(listener, store, shutdown)
            .await
            .map_err(|error| error.to_string())
    })
}

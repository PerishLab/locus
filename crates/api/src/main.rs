use clap::{Args, Parser, Subcommand};
use locus_api::column::Columns;
use locus_api::drain::Drain;
use locus_api::migrate;
use locus_api::registry::Registry;
use locus_api::retention::{self, Retention};
use locus_api::server::{self, Shared};
use locus_api::store::Store;
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
    #[arg(
        long,
        help = "Directory holding the store, registry and rejected records [default: the user's .locus]"
    )]
    home: Option<PathBuf>,
    #[arg(
        long,
        default_value = "127.0.0.1:43308",
        help = "Loopback address to serve on"
    )]
    listen: SocketAddr,
    #[arg(
        long,
        value_name = "MILLISECONDS",
        default_value_t = 1000,
        help = "Milliseconds between drain cycles"
    )]
    interval: u64,
}

fn main() -> ExitCode {
    if let Err(error) = plumb::identity!("LOCUS") {
        eprintln!("locus-api: {error}");
        return ExitCode::from(1);
    }
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
    let home = serve
        .home
        .or_else(|| plumb::config::data("locus"))
        .ok_or("no home: pass --home or set the user's home")?;
    if !serve.listen.ip().is_loopback() {
        return Err(format!("{} is not a loopback address", serve.listen));
    }
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    let columns = runtime.block_on(Columns::open(&home, runtime.handle().clone()))?;
    let store: Arc<dyn Store> = Arc::new(columns);
    let legacy = home.join("store");
    if legacy.is_dir() {
        let days = migrate::migrate(&legacy, store.as_ref())?;
        eprintln!("locus-api: migrated {days} stored day(s) into the column store");
    }
    let registry = Arc::new(Registry::open(home.join("spools.json"))?);
    let retention = Retention::read(&home)?;
    let drain = Drain::new(store.clone(), home.join("rejected"), registry.clone());
    let kept = store.clone();
    let interval = Duration::from_millis(serve.interval);
    thread::spawn(move || {
        loop {
            if let Err(error) = drain.cycle() {
                eprintln!("locus-api: drain: {error}");
            }
            let now = retention::now();
            if let Err(error) = kept.tick(now) {
                eprintln!("locus-api: seal: {error}");
            }
            if let Err(error) = retention.apply(kept.as_ref(), now) {
                eprintln!("locus-api: retention: {error}");
            }
            thread::sleep(interval);
        }
    });
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(serve.listen)
            .await
            .map_err(|error| format!("cannot listen on {}: {error}", serve.listen))?;
        let shutdown = async {
            let _ = tokio::signal::ctrl_c().await;
        };
        server::serve(listener, Shared::new(store, registry, retention), shutdown)
            .await
            .map_err(|error| error.to_string())
    })
}

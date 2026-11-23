mod access;
mod cycle;
mod doctor;
mod home;
mod observation;
mod producers;
use clap::{Args, Parser, Subcommand};
use locus_api::access::Readers;
use locus_api::column::Columns;
use locus_api::drain::Drain;
use locus_api::registry::Registry;
use locus_api::retention::Retention;
use locus_api::server::{self, Shared};
use locus_api::store::Store;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "locus-api", version = plumb::version!("LOCUS"), after_help = "Observation is off by default. Set [locus] enabled = true in config.toml under the platform .locus home, separate from locus-api.toml server retention. export-openapi buffers to state/locus there and registers it with endpoint, else LOCUS_API, else http://127.0.0.1:43308. serve records its migrate, drain, seal and retention cycles into state/locus-api under --home and never registers that buffer with any endpoint, so its own records never re-enter its drain. Observation failures go to stderr and never change output, exit status or serving.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Grant or revoke machine access to one producer in the local estate")]
    Access(access::Access),
    #[command(about = "Record product Atoms faithfully and serve them back")]
    Serve(Serve),
    #[command(name = "export-openapi", about = "Print the OpenAPI document")]
    Openapi,
    #[command(about = "Check the binary, home and live service read-only and print a JSON report")]
    Doctor(Doctor),
    #[command(about = "Read the local alert evaluator and delivery state")]
    Alerts {
        #[arg(long)]
        home: Option<PathBuf>,
    },
}

#[derive(Args)]
struct Doctor {
    #[arg(
        long,
        help = "Directory holding the store, registry and rejected records [default: the user's .locus]"
    )]
    home: Option<PathBuf>,
    #[arg(
        long,
        default_value = "127.0.0.1:43308",
        help = "Loopback address the service serves on"
    )]
    listen: SocketAddr,
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
        Command::Alerts { home } => alert(home),
        Command::Serve(serve) => match run(serve) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("locus-api: {error}");
                ExitCode::from(2)
            }
        },
        Command::Access(access) => match access.run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("locus-api: {error}");
                ExitCode::from(2)
            }
        },
        Command::Doctor(doctor) => diagnose(doctor),
        Command::Openapi => {
            let observer = observation::start("api.export-openapi");
            export();
            if let Some(observer) = observer {
                observer.finish(0);
            }
            ExitCode::SUCCESS
        }
    }
}

fn alert(home: Option<PathBuf>) -> ExitCode {
    let result = home
        .or_else(|| plumb::config::data("locus"))
        .ok_or_else(|| "no alert home".to_owned())
        .and_then(|path| server::alert::status(&path));
    match result {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("locus-api: {error}");
            ExitCode::from(2)
        }
    }
}

fn diagnose(doctor: Doctor) -> ExitCode {
    let Some(home) = doctor.home.or_else(|| plumb::config::data("locus")) else {
        eprintln!("locus-api: no home: pass --home or set the user's home");
        return ExitCode::from(2);
    };
    let report = doctor::run(plumb::version!("LOCUS"), &home, doctor.listen);
    let text = match serde_json::to_string_pretty(&report) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("locus-api: {error}");
            return ExitCode::from(2);
        }
    };
    println!("{text}");
    if report.failed() {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
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
    let columns = Arc::new(columns);
    let store: Arc<dyn Store> = columns.clone();
    let edge = server::Authority::load(&home)?;
    let alerts = server::alert::Alerts::open(&home, edge.as_ref().map(|(_, authority)| authority))?;
    let observer = observation::Watch::start(home.join("state/locus-api"));
    let legacy = home.join("store");
    if legacy.is_dir() {
        cycle::migrate(observer.as_ref(), &legacy, store.as_ref())?;
    }
    let registry = Arc::new(Registry::open(home.join("spools.json"))?);
    let retention = Retention::read(&home)?;
    let readers = Readers::read(&home)?;
    let work = cycle::Cycle {
        drain: Drain::new(store.clone(), home.join("rejected"), registry.clone()),
        store: store.clone(),
        retention,
        observer,
    };
    let interval = Duration::from_millis(serve.interval);
    thread::spawn(move || {
        loop {
            work.turn();
            thread::sleep(interval);
        }
    });
    runtime.block_on(async move {
        if let Some(alerts) = alerts {
            alerts.spawn(columns.clone());
        }
        let listener = tokio::net::TcpListener::bind(serve.listen)
            .await
            .map_err(|error| format!("cannot listen on {}: {error}", serve.listen))?;
        let (end, signal) = tokio::sync::watch::channel(false);
        let mut local = signal.clone();
        let mut remote = signal;
        let shared = Shared::new(store, registry, retention).readers(readers);
        let serving = server::serve(listener, shared, async move {
            let _ = local.changed().await;
        });
        let machine = match edge {
            Some((address, authority)) => Some((
                tokio::net::TcpListener::bind(address)
                    .await
                    .map_err(|e| e.to_string())?,
                server::edge(columns, authority, retention),
            )),
            None => None,
        };
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            let _ = end.send(true);
        });
        if let Some((listener, router)) = machine {
            let remote = axum::serve(listener, router).with_graceful_shutdown(async move {
                let _ = remote.changed().await;
            });
            tokio::try_join!(serving, std::future::IntoFuture::into_future(remote))
                .map_err(|e| e.to_string())?;
            return Ok(());
        }
        serving.await.map_err(|error| error.to_string())
    })
}

#[locus::trace(with = plumb::observation::view())]
fn export() {
    println!("{}", server::document());
}

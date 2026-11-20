use crate::store::{Filter, Producer, Record, Sink, Store};
use axum::http::StatusCode;
use keel::adapt::db::Sqlite;
use keel::atom::string;
use keel::{Core, Graph, bind, bootstrap};

mod grant;
pub use grant::{Binding, Mode, assign};

#[keel::resource(frozen)]
struct Actor {
    #[field(string)]
    issuer: string,
    #[field(string,unique=issuer)]
    subject: string,
}
use locus::{Atom, Role};
use std::path::Path;
use std::sync::Arc;
use tokio::runtime::Handle;

const DAY: u64 = 86_400_000_000_000;
const PRODUCER: &str = "producer";
const TRACE: &str = "trace";

pub struct Columns {
    core: Arc<Core<Sqlite>>,
    store: keel_column::Store<Sqlite>,
    handle: Handle,
}

impl Columns {
    pub async fn open(home: &Path, handle: Handle) -> Result<Self, String> {
        let core = core(home).await?;
        let table = keel_column::Table::new("at", &[PRODUCER, TRACE])
            .and_then(|table| table.sort(&[TRACE]))
            .and_then(|table| table.grain(Some(PRODUCER), DAY))
            .map_err(text)?;
        let store = keel_column::Store::adopt(core.clone(), table, &home.join("column"))
            .await
            .map_err(text)?;
        Ok(Self {
            core,
            store,
            handle,
        })
    }
}

impl Store for Columns {
    fn append(&self, producer: &Producer, records: &[Record], token: &str) -> Result<(), String> {
        let rows = rows(producer, records);
        self.handle
            .block_on(self.store.append(token, rows))
            .map_err(text)
    }

    fn read(&self, filter: &Filter, sink: &mut Sink<'_>) -> Result<(), String> {
        pollster::block_on(self.visit(None, filter, sink)).map_err(text)
    }

    fn retain(&self, floor: u64, now: u64) -> Result<(), String> {
        self.handle
            .block_on(self.store.retain(floor, now))
            .map_err(text)
    }

    fn tick(&self, now: u64) -> Result<(), String> {
        self.handle
            .block_on(async {
                self.store.seal(now).await?;
                self.store.sweep(now).await
            })
            .map_err(text)
    }
}

fn rows(producer: &Producer, records: &[Record]) -> Vec<keel_column::Record> {
    records
        .iter()
        .map(|record| keel_column::Record {
            raw: record.bytes().to_vec(),
            time: record.at(),
            keys: vec![producer.text().to_string(), record.trace().to_string()],
        })
        .collect()
}

impl Columns {
    pub(crate) async fn operator(
        &self,
        owner: &crate::server::identity::Owner,
    ) -> Result<i64, StatusCode> {
        grant::actor(&self.core, &owner.issuer, &owner.subject)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
            .ok_or(StatusCode::FORBIDDEN)
    }
    pub(crate) async fn submit(
        &self,
        who: i64,
        producer: &Producer,
        records: &[Record],
        token: &str,
    ) -> Result<(), keel_column::Error> {
        self.store
            .of(who, producer.text())
            .append(token, rows(producer, records))
            .await
    }
    pub(crate) async fn scan(
        &self,
        who: i64,
        filter: &Filter,
        sink: &mut Sink<'_>,
    ) -> Result<(), keel_column::Error> {
        self.visit(Some(who), filter, sink).await
    }

    async fn visit(
        &self,
        who: Option<i64>,
        filter: &Filter,
        sink: &mut Sink<'_>,
    ) -> Result<(), keel_column::Error> {
        let mut keys = Vec::new();
        if let Some(producer) = &filter.producer {
            keys.push((PRODUCER.to_string(), producer.text().to_string()));
        }
        if let Some((role, key)) = &filter.selector
            && role == &Role::trace()
        {
            keys.push((TRACE.to_string(), key.text().to_string()));
        }
        let narrowed = keel_column::Filter {
            from: filter.window.from,
            to: filter.window.to,
            keys,
        };
        let mut line = Vec::new();
        let mut admit = |raw: &[u8]| {
            let atom: Atom = serde_json::from_slice(raw)
                .map_err(|error| fault(format!("stored record is invalid: {error}")))?;
            if filter.admits(&atom) {
                line.clear();
                line.extend_from_slice(raw);
                line.push(b'\n');
                sink(&line).map_err(fault)?;
            }
            Ok(())
        };
        if let Some(who) = who {
            let source = filter
                .producer
                .as_ref()
                .ok_or_else(|| fault("source required".into()))?;
            self.store
                .of(who, source.text())
                .scan(&narrowed, &mut admit)
                .await
        } else {
            self.store.scan(&narrowed, &mut admit).await
        }
    }
}

async fn core(home: &Path) -> Result<Arc<Core<Sqlite>>, String> {
    let path = home.join("estate.sqlite3");
    let fresh = !path.exists();
    let wire = Sqlite::file(&path).await.map_err(text)?;
    let estate = if fresh {
        let mut boot = bootstrap(graph(), wire).map_err(text)?;
        let token = boot.mint().await.map_err(text)?;
        boot.seal(&token).await.map_err(text)?
    } else {
        bind(graph(), wire).await.map_err(text)?
    };
    Ok(Arc::new(estate))
}

fn graph() -> Graph {
    let mut graph = Graph::new();
    keel_column::stock(&mut graph);
    graph.plug::<Actor>().plug::<keel_column::Channel>();
    graph
}

fn fault(message: String) -> keel_column::Error {
    std::io::Error::other(message).into()
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

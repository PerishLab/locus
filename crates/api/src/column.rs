use crate::store::{Filter, Producer, Record, Sink, Store};
use keel::adapt::db::Sqlite;
use keel::{Graph, bind, bootstrap};
use locus::{Atom, Role};
use std::path::Path;
use std::sync::Arc;
use tokio::runtime::Handle;

const DAY: u64 = 86_400_000_000_000;
const PRODUCER: &str = "producer";
const TRACE: &str = "trace";

pub struct Columns {
    store: keel_column::Store<Sqlite>,
    handle: Handle,
}

impl Columns {
    pub async fn open(home: &Path, handle: Handle) -> Result<Self, String> {
        let path = home.join("estate.sqlite3");
        let fresh = !path.exists();
        let wire = Sqlite::file(&path).await.map_err(text)?;
        let core = if fresh {
            let mut boot = bootstrap(graph(), wire).map_err(text)?;
            let token = boot.mint().await.map_err(text)?;
            boot.seal(&token).await.map_err(text)?
        } else {
            bind(graph(), wire).await.map_err(text)?
        };
        let table = keel_column::Table::new("at", &[PRODUCER, TRACE])
            .and_then(|table| table.sort(&[TRACE]))
            .and_then(|table| table.grain(Some(PRODUCER), DAY))
            .map_err(text)?;
        let store = keel_column::Store::open(Arc::new(core), table, &home.join("column"))
            .await
            .map_err(text)?;
        Ok(Self { store, handle })
    }
}

impl Store for Columns {
    fn append(&self, producer: &Producer, records: &[Record], token: &str) -> Result<(), String> {
        let rows = records
            .iter()
            .map(|record| keel_column::Record {
                raw: record.bytes().to_vec(),
                time: record.at(),
                keys: vec![producer.text().to_string(), record.trace().to_string()],
            })
            .collect();
        self.handle
            .block_on(self.store.append(token, rows))
            .map_err(text)
    }

    fn read(&self, filter: &Filter, sink: &mut Sink<'_>) -> Result<(), String> {
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
        pollster::block_on(self.store.scan(&narrowed, &mut admit)).map_err(text)
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

fn graph() -> Graph {
    let mut graph = Graph::new();
    keel_column::stock(&mut graph);
    graph
}

fn fault(message: String) -> keel_column::Error {
    std::io::Error::other(message).into()
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

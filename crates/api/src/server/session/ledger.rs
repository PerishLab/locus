use axum::http::StatusCode;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, oneshot};

const LIMIT: usize = 2048;
type Reply = Result<Option<Entry>, StatusCode>;
type Map = HashMap<String, (u64, Entry)>;

#[derive(Clone)]
pub(super) enum Entry {
    Pending { verifier: String, nonce: String },
    Person(String),
}

enum Action {
    Put(String, u64, Entry),
    Take(String),
    Get(String),
}

#[derive(Clone)]
pub(super) struct Ledger(mpsc::Sender<(Action, oneshot::Sender<Reply>)>);

impl Ledger {
    pub fn open() -> Self {
        let (sender, mut receiver) = mpsc::channel(32);
        tokio::spawn(async move {
            let mut entries = Map::new();
            while let Some((action, reply)) = receiver.recv().await {
                let reply: oneshot::Sender<Reply> = reply;
                let _ = reply.send(apply(&mut entries, action));
            }
        });
        Self(sender)
    }

    pub async fn put(&self, key: String, expires: u64, entry: Entry) -> Result<(), StatusCode> {
        self.call(Action::Put(key, expires, entry))
            .await
            .map(|_| ())
    }

    pub async fn take(&self, key: &str) -> Reply {
        self.call(Action::Take(key.to_owned())).await
    }

    pub async fn get(&self, key: &str) -> Reply {
        self.call(Action::Get(key.to_owned())).await
    }

    async fn call(&self, action: Action) -> Reply {
        let (send, recv) = oneshot::channel();
        self.0
            .send((action, send))
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        recv.await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    }
}

pub(super) fn now() -> Result<u64, StatusCode> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}

fn apply(entries: &mut Map, action: Action) -> Reply {
    let time = now()?;
    match action {
        Action::Put(key, expires, entry) => {
            entries.retain(|_, (until, _)| *until > time);
            if entries.len() >= LIMIT || entries.contains_key(&key) || expires <= time {
                return Err(StatusCode::SERVICE_UNAVAILABLE);
            }
            entries.insert(key, (expires, entry));
            Ok(None)
        }
        Action::Take(key) => Ok(entries
            .remove(&key)
            .filter(|(until, _)| *until > time)
            .map(|(_, e)| e)),
        Action::Get(key) => Ok(entries
            .get(&key)
            .filter(|(until, _)| *until > time)
            .map(|(_, e)| e.clone())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired() {
        let mut map = Map::new();
        map.insert("old".into(), (1, Entry::Person("alice".into())));
        assert!(
            apply(&mut map, Action::Get("old".into()))
                .unwrap()
                .is_none()
        );
        assert!(
            apply(&mut map, Action::Take("old".into()))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn once() {
        let mut map = Map::new();
        apply(
            &mut map,
            Action::Put(
                "key".into(),
                now().unwrap() + 100,
                Entry::Person("alice".into()),
            ),
        )
        .unwrap();
        assert!(matches!(
            apply(&mut map, Action::Get("key".into())).unwrap(),
            Some(Entry::Person(_))
        ));
        assert!(
            apply(&mut map, Action::Take("key".into()))
                .unwrap()
                .is_some()
        );
        assert!(
            apply(&mut map, Action::Get("key".into()))
                .unwrap()
                .is_none()
        );
    }
}

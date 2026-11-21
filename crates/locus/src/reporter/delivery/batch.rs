use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};

const LIMIT: usize = 256 * 1024;
const LINE: usize = 1024 * 1024;

pub(super) struct Batch {
    last: i64,
    pub(super) bytes: Vec<u8>,
    pub(super) token: String,
}

impl Batch {
    pub(super) fn open(db: &mut Connection) -> Result<Option<Self>, String> {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(message)?;
        let held = tx
            .query_row("SELECT last,token FROM flight", [], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .optional()
            .map_err(message)?;
        let ceiling = held.as_ref().map_or(i64::MAX, |(last, _)| *last);
        let Some(batch) = select(&tx, ceiling)? else {
            if held.is_some() {
                return Err("in-flight HTTPS batch missing; retained".into());
            }
            return Ok(None);
        };
        if let Some((last, token)) = held {
            if batch.last != last || batch.token != token {
                return Err("in-flight HTTPS batch has changed; retained".into());
            }
        } else {
            tx.execute(
                "INSERT INTO flight VALUES(1,?1,?2)",
                rusqlite::params![batch.last, batch.token],
            )
            .map_err(message)?;
        }
        tx.commit().map_err(message)?;
        Ok(Some(batch))
    }

    pub(super) fn confirm(self, db: &mut Connection) -> Result<usize, String> {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(message)?;
        let active = tx
            .execute(
                "DELETE FROM flight WHERE last=?1 AND token=?2",
                rusqlite::params![self.last, self.token],
            )
            .map_err(message)?;
        if active != 1 {
            return Err("in-flight HTTPS confirmation changed; retained".into());
        }
        let count = tx
            .execute("DELETE FROM records WHERE sequence <= ?1", [self.last])
            .map_err(message)?;
        tx.commit().map_err(message)?;
        Ok(count)
    }
}

fn select(db: &Connection, ceiling: i64) -> Result<Option<Batch>, String> {
    let mut query = db
        .prepare(
            "SELECT sequence,body FROM records WHERE sequence <= ?1 ORDER BY sequence LIMIT 512",
        )
        .map_err(message)?;
    let mut rows = query.query([ceiling]).map_err(message)?;
    let mut last = 0;
    let mut bytes = Vec::new();
    while let Some(row) = rows.next().map_err(message)? {
        let body = row
            .get_ref(1)
            .map_err(message)?
            .as_blob()
            .map_err(|_| "invalid HTTPS Atom")?;
        if !(1..=LINE).contains(&body.len()) || body.last() != Some(&b'\n') {
            return Err("invalid HTTPS Atom; batch retained".into());
        }
        if !bytes.is_empty() && bytes.len() + body.len() > LIMIT {
            break;
        }
        last = row.get(0).map_err(message)?;
        bytes.extend_from_slice(body);
    }
    if bytes.is_empty() {
        return Ok(None);
    }
    let token = format!("{:x}", Sha256::digest(&bytes));
    Ok(Some(Batch { last, bytes, token }))
}

fn message(error: rusqlite::Error) -> String {
    format!("cannot recover HTTPS batch: {error}")
}

#[cfg(test)]
mod tests {
    use super::super::journal::{Journal, open};
    use super::*;
    #[test]
    fn recovery() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::open(dir.path()).unwrap();
        journal.append(b"one\n").unwrap();
        let mut db = open(dir.path(), false).unwrap();
        let first = Batch::open(&mut db).unwrap().unwrap();
        journal.append(b"two\n").unwrap();
        drop(db);
        let mut db = open(dir.path(), false).unwrap();
        let retry = Batch::open(&mut db).unwrap().unwrap();
        assert_eq!(first.bytes, retry.bytes);
        assert_eq!(first.token, retry.token);
        assert_eq!(retry.confirm(&mut db).unwrap(), 1);
        let next = Batch::open(&mut db).unwrap().unwrap();
        assert_eq!(next.bytes, b"two\n");
        assert_eq!(next.confirm(&mut db).unwrap(), 1);
        assert!(Batch::open(&mut db).unwrap().is_none());
    }
    #[test]
    fn ceiling() {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::open(dir.path()).unwrap();
        let mut db = open(dir.path(), false).unwrap();
        let large = [vec![b'x'; LIMIT], vec![b'\n']].concat();
        for body in [&b"one\n"[..], large.as_slice(), b"two\n"] {
            journal.append(body).unwrap();
        }
        for body in [&b"one\n"[..], large.as_slice(), b"two\n"] {
            let batch = Batch::open(&mut db).unwrap().unwrap();
            assert_eq!(batch.bytes, body);
            assert_eq!(batch.confirm(&mut db).unwrap(), 1);
        }
    }
}

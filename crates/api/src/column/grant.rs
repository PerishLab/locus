use super::{core, text};
use crate::store::Producer;
use keel::adapt::db::Sqlite;
use keel::{Core, Op, form};
use std::path::Path;

pub async fn assign(
    home: &Path,
    binding: &Binding,
    producer: &Producer,
    mode: Mode,
) -> Result<(), String> {
    if binding.subject.is_empty() {
        return Err("subject must be nonempty".into());
    }
    crate::server::identity::issuer(&binding.issuer)?;
    let estate = core(home).await?;
    estate
        .sudo()
        .batch(async |tx| {
            let actor = form("Actor").when("issuer", Op::Eq, &binding.issuer).when(
                "subject",
                Op::Eq,
                &binding.subject,
            );
            let actor = match tx.one(&actor).await? {
                Some(row) => row.key(),
                None if binding.grant => {
                    tx.put(
                        "Actor",
                        &[("issuer", &binding.issuer), ("subject", &binding.subject)],
                    )
                    .await?
                }
                None => return Ok(()),
            };
            let channel = form("Channel")
                .when("source", Op::Eq, producer.text())
                .when("mode", Op::Eq, mode.name());
            let channel = match tx.one(&channel).await? {
                Some(row) => row.key(),
                None if binding.grant => {
                    tx.put(
                        "Channel",
                        &[("source", producer.text()), ("mode", mode.name())],
                    )
                    .await?
                }
                None => return Ok(()),
            };
            let who = actor.to_string();
            let scope = format!("row {channel}");
            let fields = [
                ("who", who.as_str()),
                ("unit", "Channel"),
                ("verb", "see"),
                ("scope", scope.as_str()),
            ];
            let mut search = form("@grant");
            for (field, value) in fields {
                search = search.when(field, Op::Eq, value);
            }
            let existing = tx.ask(&search).await?;
            if binding.grant && existing.rows().is_empty() {
                tx.put("@grant", &fields).await?;
            } else if !binding.grant {
                for row in existing.rows() {
                    tx.end("@grant", row.key()).await?;
                }
            }
            Ok(())
        })
        .await
        .map_err(text)
}

pub struct Binding {
    pub issuer: String,
    pub subject: String,
    pub grant: bool,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Mode {
    Append,
    Scan,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Append => "append",
            Self::Scan => "scan",
        }
    }
}

pub(super) async fn actor(
    core: &Core<Sqlite>,
    issuer: &str,
    subject: &str,
) -> Result<Option<i64>, String> {
    core.one(
        &form("Actor")
            .when("issuer", Op::Eq, issuer)
            .when("subject", Op::Eq, subject),
    )
    .await
    .map(|r| r.map(|row| row.key()))
    .map_err(text)
}

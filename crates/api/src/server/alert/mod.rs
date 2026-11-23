mod ledger;
mod rule;
mod send;
mod state;
#[cfg(test)]
mod tests;

use super::identity::{Authority, Owner};
use crate::column::Columns;
use crate::retention;
use crate::store::{Filter, Producer, Window};
use axum::http::StatusCode;
use ledger::{Delivery, Ledger, Notice, Seat};
use rule::{Rule, Rules, SECOND};
use state::{Phase, Proof, State};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

pub struct Alerts {
    settings: Rules,
    issuer: String,
    seat: Seat,
    ledger: Ledger,
    delivery: Option<JoinHandle<Notice>>,
}

impl Alerts {
    pub fn open(home: &Path, authority: Option<&Authority>) -> Result<Option<Self>, String> {
        let Some(settings) = Rules::read(home)? else {
            return Ok(None);
        };
        let authority = authority.ok_or("alerts require the Ensign listener")?;
        let seat = Seat::open(home)?;
        let mut ledger = seat.load()?;
        ledger.interval = settings.interval * SECOND;
        ledger.delay = settings.delay * SECOND;
        ledger
            .rules
            .retain(|name, _| settings.rules.iter().any(|rule| &rule.name == name));
        for rule in &settings.rules {
            let state = ledger
                .rules
                .entry(rule.name.clone())
                .or_insert_with(|| State::new(rule, settings.interval, settings.delay));
            if state.revision > rule.revision
                || (state.revision == rule.revision
                    && state.fingerprint != rule.fingerprint(settings.interval, settings.delay))
            {
                return Err(format!("alert {} needs a new revision", rule.name));
            }
            if state.revision < rule.revision {
                *state = State::new(rule, settings.interval, settings.delay);
            }
        }
        seat.save(&ledger)?;
        Ok(Some(Self {
            settings,
            issuer: authority.issuer().into(),
            seat,
            ledger,
            delivery: None,
        }))
    }

    pub fn spawn(self, columns: Arc<Columns>) {
        tokio::spawn(async move {
            if let Err(error) = self.run(columns).await {
                eprintln!("locus-api: alert evaluator stopped: {error}");
            }
        });
    }

    async fn run(mut self, columns: Arc<Columns>) -> Result<(), String> {
        let mut timer = tokio::time::interval(Duration::from_millis(200));
        loop {
            timer.tick().await;
            let now = retention::now();
            self.evaluate(&columns, now).await?;
            self.dispatch(&columns, now).await?;
        }
    }

    async fn evaluate(&mut self, columns: &Arc<Columns>, now: u64) -> Result<(), String> {
        let step = self.ledger.interval;
        let end = now.saturating_sub(self.ledger.delay) / step * step;
        let start = end.saturating_sub(step);
        for rule in &self.settings.rules {
            let state = self.ledger.rules.get_mut(&rule.name).expect("loaded alert");
            if state.evaluated >= end {
                continue;
            }
            let stopped = state.evaluated != 0 && state.evaluated < start;
            let mut notices = Vec::new();
            if stopped {
                let gap = Proof::missing(state.evaluated, start, "stopped");
                state.stopped(start.saturating_sub(1));
                notices.extend(send::prepare(&self.settings, rule, Phase::Unknown, &gap));
            }
            let proof = observe(columns, self.owner(&rule.subject), rule.query(start, end)).await;
            let state = self.ledger.rules.get_mut(&rule.name).expect("loaded alert");
            if let Some(phase) = state.advance(rule, &proof) {
                notices.extend(send::prepare(&self.settings, rule, phase, &proof));
            }
            self.ledger.room(notices.len())?;
            self.ledger.notices.extend(notices);
            self.seat.save(&self.ledger)?;
        }
        Ok(())
    }

    fn owner(&self, subject: &str) -> Owner {
        Owner {
            issuer: self.issuer.clone(),
            subject: subject.to_owned(),
        }
    }

    async fn dispatch(&mut self, columns: &Columns, now: u64) -> Result<(), String> {
        if self
            .delivery
            .as_ref()
            .is_some_and(|task| task.is_finished())
        {
            let result = self
                .delivery
                .take()
                .expect("finished delivery")
                .await
                .map_err(|error| error.to_string())?;
            if let Some(notice) = self
                .ledger
                .notices
                .iter_mut()
                .find(|entry| entry.id == result.id)
            {
                *notice = result;
                self.seat.save(&self.ledger)?;
            }
        }
        if self.delivery.is_some() {
            return Ok(());
        }
        let Some(index) = self
            .ledger
            .notices
            .iter()
            .position(|item| item.delivery == Delivery::Pending && item.retry <= now)
        else {
            return Ok(());
        };
        let destination = destination(&self.settings.rules, &self.ledger.notices[index], now);
        let entry = &self.ledger.notices[index];
        let owner = self.owner(&entry.recipient);
        let available = permitted(columns, owner, &entry.producer).await;
        let mutable = &mut self.ledger.notices[index];
        match (destination, available) {
            (Destination::Ready(endpoint), Ok(())) => {
                let mut entry = mutable.clone();
                let endpoint = endpoint.to_owned();
                self.delivery = Some(tokio::task::spawn_blocking(move || {
                    send::deliver(&send::agent(), &endpoint, &mut entry, now);
                    entry
                }));
            }
            (Destination::Muted, _) => mutable.delivery = Delivery::Muted,
            (Destination::Absent, _) | (_, Err(StatusCode::FORBIDDEN)) => {
                mutable.delivery = Delivery::Refused
            }
            (_, Err(_)) => send::postpone(mutable, now),
        }
        self.seat.save(&self.ledger)
    }
}

enum Destination<'a> {
    Ready(&'a str),
    Muted,
    Absent,
}

fn destination<'a>(rules: &'a [Rule], notice: &Notice, now: u64) -> Destination<'a> {
    let Some(rule) = rules
        .iter()
        .find(|rule| rule.name == notice.rule && rule.revision == notice.revision)
    else {
        return Destination::Absent;
    };
    if rule.mute.is_some_and(|until| now < until) {
        return Destination::Muted;
    }
    rule.recipients
        .iter()
        .find(|person| person.subject == notice.recipient)
        .map_or(Destination::Absent, |person| {
            Destination::Ready(person.endpoint.as_str())
        })
}

async fn observe(
    columns: &Arc<Columns>,
    owner: Owner,
    query: locus_derivation::metric::Query,
) -> Proof {
    let window = (query.from, query.to);
    let Ok(who) = columns.operator(&owner).await else {
        return Proof::missing(window.0, window.1, "authorization");
    };
    match super::metric::evaluate(columns.clone(), who, query).await {
        Ok(report) => Proof::report(&report),
        Err(StatusCode::FORBIDDEN) => Proof::missing(window.0, window.1, "authorization"),
        Err(_) => Proof::missing(window.0, window.1, "read_failure"),
    }
}

async fn permitted(columns: &Columns, owner: Owner, name: &str) -> Result<(), StatusCode> {
    let operator = columns.operator(&owner).await?;
    let producer = Producer::new(name).map_err(|_| StatusCode::FORBIDDEN)?;
    let filter = Filter {
        window: Window {
            from: Some(0),
            to: Some(0),
        },
        producer: Some(producer),
        selector: None,
    };
    columns
        .scan(operator, &filter, &mut |_| Ok(()))
        .await
        .map_err(super::ingress::status)
}

pub fn status(home: &Path) -> Result<serde_json::Value, String> {
    ledger::status(home, retention::now())
}

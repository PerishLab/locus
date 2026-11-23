use super::ledger::{Delivery, Notice};
use super::rule::{Rule, Rules, SECOND};
use super::state::{Phase, Proof};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub(super) fn prepare(rules: &Rules, rule: &Rule, phase: Phase, proof: &Proof) -> Vec<Notice> {
    let mut link = rules.origin.clone();
    link.set_path("/api/v1/metrics");
    link.query_pairs_mut().extend_pairs(parameters(rule, proof));
    rule.recipients
        .iter()
        .map(|recipient| {
            let identity = format!(
                "{}\0{}\0{}\0{:?}\0{}",
                rule.name, rule.revision, proof.to, phase, recipient.subject
            );
            let delivery = if rule.mute.is_some_and(|until| proof.to < until) {
                Delivery::Muted
            } else {
                Delivery::Pending
            };
            Notice {
                id: format!("{:x}", Sha256::digest(identity.as_bytes())),
                rule: rule.name.clone(),
                revision: rule.revision,
                producer: rule.producer.clone(),
                recipient: recipient.subject.clone(),
                phase,
                proof: proof.clone(),
                link: link.to_string(),
                delivery,
                attempts: 0,
                retry: proof.to,
                http: None,
            }
        })
        .collect()
}

fn parameters(rule: &Rule, proof: &Proof) -> [(&'static str, String); 7] {
    let kind = match rule.kind {
        locus_derivation::metric::Kind::Gauge => "gauge",
        locus_derivation::metric::Kind::Counter => "counter",
    };
    let start = proof.from.max(proof.to.saturating_sub(7 * 86400 * SECOND));
    [
        ("producer", rule.producer.clone()),
        ("name", rule.metric.clone()),
        ("kind", kind.into()),
        ("unit", rule.unit.clone()),
        ("from", start.to_string()),
        ("to", proof.to.to_string()),
        ("step", (proof.to - start).to_string()),
    ]
}

pub(super) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .new_agent()
}

pub(super) fn deliver(agent: &ureq::Agent, endpoint: &str, notice: &mut Notice, now: u64) {
    let reply = agent
        .post(endpoint)
        .header("Idempotency-Key", &notice.id)
        .send_json(&*notice);
    notice.attempts = notice.attempts.saturating_add(1);
    notice.http = reply.ok().map(|response| response.status().as_u16());
    if notice
        .http
        .is_some_and(|status| (200..300).contains(&status))
    {
        notice.delivery = Delivery::Sent;
    } else {
        postpone(notice, now);
    }
}

pub(super) fn postpone(notice: &mut Notice, now: u64) {
    notice.retry = now.saturating_add(2_u64.pow(notice.attempts.min(8)) * SECOND);
}

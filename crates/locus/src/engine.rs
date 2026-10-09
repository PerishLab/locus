use crate::atom::{Accepted, Atom, Candidate, Choice, Collection, Header, Origin};
use crate::collector::{self, Builtin};
use crate::generator::{self, Generate};
use crate::hook::{Adaptor, Diagnostic, Hook, Observation, Outcome, Status};
use crate::policy::{Config, Policy};
use crate::reporter::{self, Report};
use crate::{Context, Error, Key, Role};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

thread_local! {
    static OBSERVING: Cell<bool> = const { Cell::new(false) };
}

pub struct Engine {
    producer: Option<String>,
    instance: u64,
    sequence: AtomicU64,
    collectors: BTreeMap<String, Vec<Builtin>>,
    enabled: bool,
    reporter: Option<(String, Box<dyn Report>)>,
    fallback: Arc<dyn Generate>,
    generators: BTreeMap<Role, Arc<dyn Generate>>,
    hook: Arc<dyn Hook>,
}

impl Engine {
    pub fn bootstrap(config: Config) -> Result<Self, Error> {
        let Config { policy, hook } = config;
        let result = Self::build(policy, hook.clone());
        if let Err(error) = &result {
            handoff(
                hook.as_ref(),
                Observation::Diagnostic(Diagnostic::new("bootstrap.failed", error.to_string())),
            );
        }
        result
    }

    fn build(policy: Policy, hook: Arc<dyn Hook>) -> Result<Self, Error> {
        let collectors = collector::bindings(&policy.collectors)?;
        let fallback = generator::build(&policy.fallback)?;
        let generators = generator::roles(&policy.generators)?;
        let reporter = if policy.enabled {
            let spec = policy
                .reporter
                .as_ref()
                .ok_or_else(|| Error::config("enabled reporting requires a reporter"))?;
            Some((spec.name().into(), reporter::build(spec)?))
        } else {
            None
        };
        if let Some(producer) = &policy.producer {
            crate::policy::admit(producer)?;
        }
        let target = reporter
            .as_ref()
            .and_then(|(_, reporter)| reporter.target());
        if let Some(Err(error)) =
            target.map(|target| reporter::enroll(&target, policy.producer.as_deref()))
        {
            handoff(
                hook.as_ref(),
                Observation::Diagnostic(Diagnostic::new("reporter.registration", error)),
            );
        }
        Ok(Self {
            producer: policy.producer.clone(),
            instance: instance()?,
            sequence: AtomicU64::new(0),
            collectors,
            enabled: policy.enabled,
            reporter,
            fallback,
            generators,
            hook,
        })
    }

    pub fn append(&self, context: &Context, mut candidate: Candidate) -> Result<Accepted, Error> {
        candidate.provenance = self.collect(&mut candidate)?;
        if candidate.payload.is_none() && candidate.needs.is_empty() {
            return Err(Error::record("candidate contains no fact"));
        }
        let mut context = context.clone();
        let mut choices = Vec::with_capacity(candidate.needs.len());
        for (role, explicit) in std::mem::take(&mut candidate.needs) {
            let (key, origin) = self.resolve(&context, &role, explicit)?;
            choices.push(Choice::new(&role, &key, origin));
            context = context.bind(role, key);
        }
        let at = match now() {
            Ok(at) => at,
            Err(error) => {
                self.diagnostic("clock.failed", &error);
                return Err(error);
            }
        };
        let header = Header {
            at,
            id: format!(
                "{:016x}{:016x}",
                self.instance,
                self.sequence.fetch_add(1, Ordering::Relaxed)
            ),
            producer: self.producer.clone(),
        };
        let atom = Atom::new(header, &context, choices, candidate);
        self.report(atom);
        Ok(Accepted::new(context))
    }

    fn collect(&self, candidate: &mut Candidate) -> Result<Vec<Collection>, Error> {
        let mut seen = BTreeMap::new();
        let mut provenance = Vec::new();
        for (role, binding) in std::mem::take(&mut candidate.requests) {
            if seen.insert(role.clone(), binding.clone()).is_some() {
                return self.refuse(format!(
                    "role {} has more than one collector binding",
                    role.text()
                ));
            }
            if matches!(candidate.needs.get(&role), Some(Some(_))) {
                continue;
            }
            if let Some((key, collection)) = self.sample(&role, &binding)? {
                candidate.needs.insert(role.clone(), Some(key));
                provenance.push(collection);
            }
        }
        Ok(provenance)
    }

    fn sample(&self, role: &Role, binding: &str) -> Result<Option<(Key, Collection)>, Error> {
        let chain = match self.collectors.get(binding) {
            Some(chain) => chain,
            None => return self.refuse(format!("unknown collector binding: {binding}")),
        };
        let sample = match collector::first(chain) {
            Ok(sample) => sample,
            Err(error) => return self.failed(error),
        };
        let Some(sample) = sample else {
            return Ok(None);
        };
        let key = match Key::new(sample.value) {
            Ok(key) => key,
            Err(error) => {
                return self.refuse(format!(
                    "collector binding {binding} produced an invalid key: {error}"
                ));
            }
        };
        Ok(Some((
            key,
            Collection::new(role, binding, sample.collector, sample.selector),
        )))
    }

    fn refuse<T>(&self, message: String) -> Result<T, Error> {
        self.failed(Error::collector(message))
    }

    fn failed<T>(&self, error: Error) -> Result<T, Error> {
        self.diagnostic("collector.failed", &error);
        Err(error)
    }

    fn resolve(
        &self,
        context: &Context,
        role: &Role,
        explicit: Option<crate::Key>,
    ) -> Result<(crate::Key, Origin), Error> {
        if let Some(key) = explicit {
            return Ok((key, Origin::Explicit));
        }
        if let Some(key) = context.read(role) {
            return Ok((key.clone(), Origin::Inherited));
        }
        let generator = self.generators.get(role).unwrap_or(&self.fallback);
        match generator.generate(role) {
            Ok(key) => Ok((key, Origin::Generated)),
            Err(error) => {
                let error =
                    Error::generator(format!("cannot resolve role {}: {error}", role.text()));
                self.diagnostic("generator.failed", &error);
                Err(error)
            }
        }
    }

    fn report(&self, atom: Atom) {
        let (outcome, loss) = self.deliver(atom);
        self.observe(Observation::Report(outcome));
        if let Some(loss) = loss {
            self.observe(Observation::Diagnostic(Diagnostic::new(
                "reporter.loss",
                loss,
            )));
        }
    }

    fn deliver(&self, atom: Atom) -> (Outcome, Option<String>) {
        let Some((name, reporter)) = self.reporter.as_ref().filter(|_| self.enabled) else {
            let (status, error) = if self.enabled {
                (
                    Status::Failed,
                    Some("enabled reporting has no reporter".into()),
                )
            } else {
                (Status::Gated, None)
            };
            return (Outcome::new(atom, None, status, error), None);
        };
        match reporter.report(&atom) {
            Ok(loss) => (
                Outcome::new(atom, Some(name.clone()), Status::Delivered, None),
                loss,
            ),
            Err(error) => (
                Outcome::new(atom, Some(name.clone()), Status::Failed, Some(error)),
                None,
            ),
        }
    }

    fn observe(&self, observation: Observation) {
        handoff(self.hook.as_ref(), observation);
    }

    fn diagnostic(&self, code: &str, error: &Error) {
        self.observe(Observation::Diagnostic(Diagnostic::new(
            code,
            error.to_string(),
        )));
    }
}

fn handoff(hook: &dyn Hook, observation: Observation) {
    OBSERVING.with(|active| {
        if active.replace(true) {
            Adaptor.observe(&Observation::Diagnostic(Diagnostic::new(
                "hook.reentrant",
                "hook observation re-entered Locus",
            )));
            return;
        }
        let result = catch_unwind(AssertUnwindSafe(|| hook.observe(&observation)));
        active.set(false);
        if result.is_err() {
            Adaptor.observe(&Observation::Diagnostic(Diagnostic::new(
                "hook.panic",
                "configured hook panicked",
            )));
        }
    });
}

fn instance() -> Result<u64, Error> {
    let mut bytes = [0_u8; 8];
    getrandom::fill(&mut bytes)
        .map_err(|error| Error::generator(format!("random generator failed: {error}")))?;
    Ok(u64::from_le_bytes(bytes))
}

fn now() -> Result<u64, Error> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::record(format!("system clock precedes Unix epoch: {error}")))?;
    u64::try_from(duration.as_nanos())
        .map_err(|_| Error::record("system time cannot fit the Atom clock"))
}

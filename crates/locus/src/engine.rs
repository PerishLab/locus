use crate::atom::{Accepted, Atom, Candidate, Choice, Collection, Origin};
use crate::collector::{self, Builtin};
use crate::generator::{self, Generate};
use crate::hook::{Adaptor, Diagnostic, Hook, Observation, Outcome, Status};
use crate::reporter::{self, Report};
use crate::{Context, Error, Key, Role};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

thread_local! {
    static OBSERVING: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone, Debug)]
pub struct Policy {
    collectors: BTreeMap<String, Vec<collector::Spec>>,
    enabled: bool,
    reporter: Option<reporter::Spec>,
    fallback: generator::Spec,
    generators: BTreeMap<Role, generator::Spec>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            collectors: BTreeMap::new(),
            enabled: false,
            reporter: None,
            fallback: generator::Spec::random(),
            generators: BTreeMap::new(),
        }
    }
}

impl Policy {
    pub fn collector(mut self, binding: impl Into<String>, spec: collector::Spec) -> Self {
        self.collectors
            .entry(binding.into())
            .or_default()
            .push(spec);
        self
    }

    pub fn reporter(mut self, spec: reporter::Spec) -> Self {
        self.enabled = true;
        self.reporter = Some(spec);
        self
    }

    pub fn fallback(mut self, spec: generator::Spec) -> Self {
        self.fallback = spec;
        self
    }

    pub fn generator(mut self, role: Role, spec: generator::Spec) -> Self {
        self.generators.insert(role, spec);
        self
    }

    pub fn disable(mut self) -> Self {
        self.enabled = false;
        self
    }
}

pub struct Config {
    policy: Policy,
    hook: Arc<dyn Hook>,
}

impl Config {
    pub fn new(policy: Policy) -> Self {
        Self {
            policy,
            hook: Arc::new(Adaptor),
        }
    }

    pub fn hook(mut self, hook: Arc<dyn Hook>) -> Self {
        self.hook = hook;
        self
    }
}

pub struct Engine {
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
        Ok(Self {
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
        let atom = Atom::new(at, &context, choices, candidate);
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
        let outcome = if !self.enabled {
            Outcome::new(atom, None, Status::Gated, None)
        } else if let Some((name, reporter)) = &self.reporter {
            match reporter.report(&atom) {
                Ok(()) => Outcome::new(atom, Some(name.clone()), Status::Delivered, None),
                Err(error) => Outcome::new(atom, Some(name.clone()), Status::Failed, Some(error)),
            }
        } else {
            Outcome::new(
                atom,
                None,
                Status::Failed,
                Some("enabled reporting has no reporter".into()),
            )
        };
        self.observe(Observation::Report(outcome));
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

fn now() -> Result<u64, Error> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::record(format!("system clock precedes Unix epoch: {error}")))?;
    u64::try_from(duration.as_nanos())
        .map_err(|_| Error::record("system time cannot fit the Atom clock"))
}

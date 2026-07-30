use crate::atom::{Accepted, Atom, Candidate, Choice, Origin};
use crate::generator::{self, Generate};
use crate::hook::{Adaptor, Diagnostic, Hook, Observation, Outcome, Status};
use crate::reporter::{self, Report};
use crate::{Context, Error, Role};
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
    enabled: bool,
    reporter: Option<reporter::Spec>,
    fallback: generator::Spec,
    generators: BTreeMap<Role, generator::Spec>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: false,
            reporter: None,
            fallback: generator::Spec::random(),
            generators: BTreeMap::new(),
        }
    }
}

impl Policy {
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
    enabled: bool,
    reporter: Option<(String, Box<dyn Report>)>,
    fallback: Arc<dyn Generate>,
    generators: BTreeMap<Role, Arc<dyn Generate>>,
    hook: Arc<dyn Hook>,
}

impl Engine {
    pub fn bootstrap(config: Config) -> Result<Self, Error> {
        let fallback = generator::build(&config.policy.fallback)?;
        let generators = generator::roles(&config.policy.generators)?;
        let reporter = if config.policy.enabled {
            let spec = config
                .policy
                .reporter
                .as_ref()
                .ok_or_else(|| Error::config("enabled reporting requires a reporter"))?;
            Some((spec.name().into(), reporter::build(spec)?))
        } else {
            None
        };
        Ok(Self {
            enabled: config.policy.enabled,
            reporter,
            fallback,
            generators,
            hook: config.hook,
        })
    }

    pub fn append(&self, context: &Context, mut candidate: Candidate) -> Result<Accepted, Error> {
        if candidate.payload.is_none() && candidate.needs.is_empty() {
            return Err(Error::record("candidate contains no fact"));
        }
        let mut context = context.clone();
        let mut choices = Vec::with_capacity(candidate.needs.len());
        for (role, explicit) in std::mem::take(&mut candidate.needs) {
            let (key, origin) = if let Some(key) = explicit {
                (key, Origin::Explicit)
            } else if let Some(key) = context.read(&role) {
                (key.clone(), Origin::Inherited)
            } else {
                let generator = self.generators.get(&role).unwrap_or(&self.fallback);
                (
                    generator.generate(&role).map_err(|error| {
                        Error::generator(format!("cannot resolve role {}: {error}", role.text()))
                    })?,
                    Origin::Generated,
                )
            };
            choices.push(Choice::new(&role, &key, origin));
            context = context.bind(role, key);
        }
        let atom = Atom::new(now()?, &context, choices, candidate);
        self.report(atom);
        Ok(Accepted::new(context))
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
        OBSERVING.with(|active| {
            if active.replace(true) {
                Adaptor.observe(&Observation::Diagnostic(Diagnostic::new(
                    "hook.reentrant",
                    "hook observation re-entered Locus",
                )));
                return;
            }
            let result = catch_unwind(AssertUnwindSafe(|| self.hook.observe(&observation)));
            active.set(false);
            if result.is_err() {
                Adaptor.observe(&Observation::Diagnostic(Diagnostic::new(
                    "hook.panic",
                    "configured hook panicked",
                )));
            }
        });
    }
}

fn now() -> Result<u64, Error> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::record(format!("system clock precedes Unix epoch: {error}")))?;
    u64::try_from(duration.as_nanos())
        .map_err(|_| Error::record("system time cannot fit the Atom clock"))
}

# Product adapter cold start

One closed loop costs about fifty deletable lines inside one crate. Do not
design a product-specific shape; the steps below already survived a second
product's own guard. Treat every line as temporary syntax intrusion and delete
the adapter whole once the report is consumed.

1. Discover the product's repository instructions and normal verification.
2. Add the dependency to the crate that holds both the seat and the traced
   declarations. The source adapter arrives with it.

```sh
cargo add --package PRODUCT locus@0.2 --registry perish
```

3. Create one isolated module carrying the seat, the gate, and the policy. Split
   the seat into a library crate only when traced declarations live there. Never
   expose Locus types through business APIs or state models.

```rust
use locus::{Candidate, Config, Context, Engine, Policy, Role, collector, reporter};
use plumb::config::Cascade;
use std::path::PathBuf;
use std::sync::OnceLock;

static SEAT: OnceLock<(Engine, Context)> = OnceLock::new();

#[derive(Debug, Default, PartialEq, Cascade)]
struct Settings {
    report: PathBuf,
}

pub(crate) fn start() {
    if let Some(seat) = load() {
        let _ = SEAT.set(seat);
    }
}

pub(crate) fn view() -> Option<(&'static Engine, &'static Context)> {
    SEAT.get().map(|(engine, context)| (engine, context))
}

fn load() -> Option<(Engine, Context)> {
    let seen = <Settings as Cascade>::env("PRODUCT_LOCUS").ok()?;
    let settings = Settings::default().merge(seen);
    if settings.report.as_os_str().is_empty() {
        return None;
    }
    build(settings).ok()
}

fn build(settings: Settings) -> Result<(Engine, Context), locus::Error> {
    let trace = Role::trace();
    let policy = Policy::default()
        .collector(
            "agent.session",
            collector::Spec::environment("CODEX_THREAD_ID", 512),
        )
        .collector(
            "agent.session",
            collector::Spec::environment("CLAUDE_CODE_SESSION_ID", 512),
        )
        .reporter(reporter::Spec::file(settings.report));
    let engine = Engine::bootstrap(Config::new(policy))?;
    let candidate = Candidate::context()
        .collect(trace.clone(), "agent.session")
        .ensure(trace);
    let context = engine.append(&Context::empty(), candidate)?.context();
    Ok((engine, context))
}
```

Read the gate through the Plumb cascade. A guarded product refuses raw
environment syntax outside its granted paths, so a hand-rolled read is a finding
rather than a shortcut.

One binding carries a chain of exact bounded collectors. The chain advances only
on absence and the default random fallback covers an unrecognized executor, so
cold start declares no generator, no shared key, and no explicit identity. Never
scan the whole environment, argv, process, or filesystem.

The presence of `PRODUCT_LOCUS_REPORT` is the entire gate. An absent path is a
muted run, and an operator outside the product owns the endpoint and retention.

4. Call `start` as the first statement of `main`.
5. Attach `#[locus::trace(with = crate::observation::view())]` to three or four
   core-path declarations. Inherent methods are declarations; inner expressions
   and blocks are not. Split a function when its declaration is too coarse.
6. Ignore observation outcomes on the business path, then prove it: run the same
   command muted and observed and require an identical exit status, stdout
   bytes, and stderr bytes.
7. Consume the report, run the product's complete guard, then delete the adapter
   whole.

```sh
locus query locus.trace < report.jsonl
locus query locus.trace KEY < report.jsonl
```

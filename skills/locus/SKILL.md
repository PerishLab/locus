---
name: locus
description: Operate Locus's readonly context and immutable observation substrate. Use when instrumenting a Rust product with transparent trace/span observations; selecting collectors, generators, reporters, or hooks; querying or inspecting Atom JSONL with the Locus CLI; diagnosing observation coverage or delivery gaps; or changing the Locus repository, locus.toml, semantic laws, vocabulary, or delivery conventions.
---

# Locus

Treat Locus as a faithful historian. Observed products produce facts; Locus
accepts, relates, reports, and queries them without gaining authority over the
products' business delivery.

## Core boundary

- Preserve semantic transparency. Removing or disabling Locus may remove
  evidence, but never business capability, state, output, exit status, or
  control flow.
- Keep Locus Context inside the observation path. Never use a trace, span,
  collection, report outcome, or query result to authorize or select business
  behavior.
- Keep product vocabulary opaque to Locus. Bind exact product-owned facts to
  roles without adding product names to Locus algorithms.
- Preserve atomic facts. Derive duration, nesting, overlap, aggregation,
  attribution, lifecycle, and liveness downstream.
- Keep observation failure visible to the audit operator and isolated from the
  observed command's result.

## Cold-start a product adapter

One closed loop costs about fifty deletable lines inside one crate. Do not
design a product-specific shape; the steps below already survived a second
product's own guard.

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
   whole. Treat every line of it as temporary syntax intrusion.

```sh
locus query locus.trace < report.jsonl
locus query locus.trace KEY < report.jsonl
```

### Cold-start refusals

- A traced declaration that reaches `std::process::exit` records `enter` and no
  `return`, exactly as a panic does. Trace below the exit boundary, or raise the
  exit into `main`.
- Frames derive from the process seat, so spans are siblings rather than a tree.
  Nesting, duration, and attribution stay downstream derivations.
- A product guard that passes before the adapter is not evidence. Rerun it with
  the adapter present.

## Query and inspect history

Consume explicit Atom JSONL through stdin:

```sh
locus query locus.trace < atoms.jsonl
locus query locus.trace TRACE_KEY < atoms.jsonl
locus query locus.span < atoms.jsonl
locus inspect /path/to/product < atoms.jsonl
```

Use `query ROLE` to enumerate sorted identities with record counts. Add an
exact key to replay matching logical Atoms in input order. Read the final
`locus.query/v1` summary for total, matched, and identity coverage. Empty or
unmatched input is a successful query with explicit zero coverage; malformed
input refuses without partial output.

Use `inspect ROOT` only when the product root carries `locus.toml`. Inspection
composes Locus-owned mappings with product-declared thresholds and emits
measurements, not cause or repair advice.

Do not interpret either command as process liveness, executor ownership,
parenthood, lifecycle, or business authority.

## Change Locus itself

1. Enter the Locus repository and read `AGENTS.md`, `docs/model/laws.md`,
   `docs/model/vocabulary.md`, and `docs/run/verify.md` as required by the
   change.
2. Use the Plumb skill and run `plumb doctor .` before changing repository
   shape. Use the Ectropy skill, read `ectropy.toml`, and run `ectropy .`.
3. Put Context, Atom, collector, generator, reporter, hook, and diagnostic
   mechanics in `crates/locus`; source adaptation in `crates/macro`; stdin-first
   consumption in `crates/cli`.
4. Write the law before or with its mechanism. Keep product analyzer identities,
   thresholds, endpoints, credentials, retention, and consumption policy out
   of the substrate.
5. Run the repository's complete guard before landing.

## Standing

Locus currently mechanizes immutable Context derivation, identity precedence,
exact bounded collectors, generated and shared keys, immutable Atom records,
function declaration tracing, file reporting, diagnostic isolation, structural
inspection, and exact role query with complete summaries.

The skill holds the still-prose obligation that downstream integration remain
semantically transparent and removable. No checker proves every product has
kept Locus out of its business delivery.

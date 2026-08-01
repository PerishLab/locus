# locus

Readonly context and immutable observation substrate. Observation is
semantically transparent: removing Locus removes evidence, not business
capability or behavior.

Locus accepts one candidate record, resolves requested semantic roles, returns
a new immutable context view, and independently drives configured reporting.
Trace and span are ordinary roles over the same record stream.

```rust
use locus::{Candidate, Config, Context, Engine, Policy, Role};
use serde_json::json;

let engine = Engine::bootstrap(Config::new(Policy::default()))?;
let context = Context::empty();
let trace = Role::trace();

let accepted = locus::record!(
    &engine,
    &context,
    Candidate::event(json!({"event": "cli.start"})).ensure(trace),
)?;

let context = accepted.context();
# Ok::<(), locus::Error>(())
```

Function observation is an explicit downstream decision. A product-owned
accessor supplies a readonly engine and context view; absence is a true no-op.

```rust
# use locus::{Config, Context, Engine, Policy};
# use std::sync::OnceLock;
# static SEAT: OnceLock<(Engine, Context)> = OnceLock::new();
fn observation() -> Option<(&'static Engine, &'static Context)> {
    SEAT.get().map(|(engine, context)| (engine, context))
}

#[locus::trace(with = observation())]
fn reconcile() -> bool {
    true
}
# let _ = Engine::bootstrap(Config::new(Policy::default()));
```

The macro emits `enter` and normal `return` source records with one inherited
trace/span context. `Err` is a normal return. Panic, abort, and async
cancellation intentionally have no terminal record. It rejects `const`,
`unsafe`, and `#[track_caller]` functions whose semantics the wrapper cannot
preserve.

The `locus` binary reads analyzer declarations from one product root and
inspects JSONL Atom structure from stdin:

```sh
locus inspect . < atoms.jsonl
```

It also queries one exact Context role without a product declaration. Omitting
the key enumerates identities; supplying the key replays matching logical
Atoms in input order:

```sh
locus query locus.trace < atoms.jsonl
locus query locus.trace TRACE_KEY < atoms.jsonl
```

Query output uses `locus.query/v1` JSONL and ends with a complete summary. It
does not infer liveness, ownership, lifecycle, parenthood, or cause.

The root carries `locus.toml`; it declares analyzer identities by composing
Locus-owned mappings with thresholds:

```toml
version = 1

[[analyzer]]
id = "trace.size"
mapping = { kind = "representation-bytes", group = "locus.trace" }
threshold = { above = 8192 }

[[analyzer]]
id = "trace.prefix"
mapping = { kind = "dominant-content-prefix-percent", group = "locus.trace" }
threshold = { above = 80 }
```

Inspection emits `locus.inspect/v1` JSONL. Findings precede one final summary
that names complete analyzer coverage, including empty coverage. Clean input
exits zero, findings exit one, and malformed declaration, input, or I/O exits
two without a partial report. Inspection never changes acceptance, reporting,
or the input stream, and it does not attribute a finding to a product or cause.

The file reporter creates new report files with private owner-only permissions
on Unix. Operators still own the explicit report path and retention policy.

Canonical source: [PerishLab/locus](https://git.perish.top/PerishLab/locus).

The cold-start contract is documented in
[`docs/run/verify.md`](docs/run/verify.md).

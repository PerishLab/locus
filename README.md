# locus

Readonly context and immutable observation substrate.

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

Canonical source: [PerishLab/locus](https://git.perish.top/PerishLab/locus).

The cold-start contract is documented in
[`docs/run/verify.md`](docs/run/verify.md).

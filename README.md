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

Canonical source: [PerishLab/locus](https://git.perish.top/PerishLab/locus).

The cold-start contract is documented in
[`docs/run/verify.md`](docs/run/verify.md).

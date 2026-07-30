# Cold-start verification

The first closed loop proves Context derivation, identity selection, one Atom
record, reporting ownership, and diagnostic isolation without defining a
storage or query engine.

## Must pass

- explicit identity wins over inherited identity and generation
- inherited identity wins over generation
- invalid identity refuses without fallback
- random generation supplies an absent role
- shared-file generation returns one stable cross-engine key
- derived Context shadows one role without changing the old view
- each accepted append produces one immutable Atom
- disabled reporting remains a valid acceptance path
- the file reporter appends one JSON record and reports its outcome to a hook
- reporter failure cannot roll back acceptance
- hook failure cannot affect the engine or re-enter reporting
- the record macro captures source without imposing a lifecycle
- format, clippy, tests, Plumb, and Ectropy are green

## Must not require

- span start, end, parent, nesting, tree, status, or duration
- an ambient runtime context
- caller-controlled report, flush, retry, or drain
- HTTP reporting
- log query, aggregation, attribution, or storage optimization
- a canonical binary codec
- OpenTelemetry compatibility or dependency

## Commands

```sh
runseal :guard
cargo test --locked --workspace
plumb doctor .
ectropy .
```

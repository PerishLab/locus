# Cold-start verification

The first closed loop proves Context derivation, identity selection, one Atom
record, reporting ownership, and diagnostic isolation without defining a
storage or query engine.

## Must pass

- explicit identity wins over inherited identity and generation
- inherited identity wins over generation
- invalid identity refuses without fallback
- callers explicitly bind built-in collectors; there is no ambient discovery
- environment, argv, and process collectors require exact bounded selectors
- collector chains fall through only on absence
- invalid or failed collection refuses and reaches the diagnostic hook
- one successful collection is frozen into Atom provenance
- random generation supplies an absent role
- shared-file generation returns one stable cross-engine key
- concurrent shared-file generation never exposes a partial stored key
- derived Context shadows one role without changing the old view
- each accepted append produces one immutable Atom
- disabled reporting remains a valid acceptance path
- the file reporter appends one JSON record and reports its outcome to a hook
- concurrent file reporters preserve complete JSONL record boundaries
- reporter failure cannot roll back acceptance
- hook failure cannot affect the engine or re-enter reporting
- the record macro captures source without imposing a lifecycle
- the function macro records entry and normal return with one context
- `Err` remains a normal return; panic and async cancellation leave only entry
- a missing downstream observation view performs no append or identity work
- unsupported `const`, `unsafe`, and `#[track_caller]` semantics are rejected
- function declarations are the minimum automatic observation boundary
- stdin JSONL inspection groups only by `locus.trace` without a lifecycle
- a trace representation beyond 8 KiB produces one size finding
- a decoded content prefix above eighty percent produces one prefix finding
- JSON syntax and the Locus envelope do not create prefix findings
- inspection findings exit one; malformed input exits two without partial clean
- format, clippy, tests, Plumb, and Ectropy are green

## Must not require

- span start, end, parent, nesting, tree, status, or duration
- inner-block or expression-level automatic tracing
- an ambient runtime context
- caller-controlled report, flush, retry, or drain
- arbitrary collector injection or default ambient scanning
- whole-environment, whole-argv, or whole-process collection
- HTTP reporting
- log query, aggregation, attribution, or storage optimization
- inspection config, product vocabulary, cause inference, or repair advice
- a canonical binary codec
- OpenTelemetry compatibility or dependency

## Commands

```sh
runseal :guard
cargo test --locked --workspace
plumb doctor .
ectropy .
```

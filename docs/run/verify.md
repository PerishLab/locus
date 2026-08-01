# Cold-start verification

The first closed loop proves Context derivation, identity selection, one Atom
record, reporting ownership, diagnostic isolation, and exact readonly query
without defining a storage engine or expression language.

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
- a newly created Unix report file is private to its owner
- reporter failure cannot roll back acceptance
- hook failure cannot affect the engine or re-enter reporting
- the record macro captures source without imposing a lifecycle
- the function macro records entry and normal return with one context
- `Err` remains a normal return; panic and async cancellation leave only entry
- a missing downstream observation view performs no append or identity work
- unsupported `const`, `unsafe`, and `#[track_caller]` semantics are rejected
- function declarations are the minimum automatic observation boundary
- root `locus.toml` strictly composes mappings and thresholds before stdin read
- representation bytes can be grouped by an explicitly declared Context role
- dominant decoded content prefix percentage is a separate mapping
- a trace representation beyond the declared 8 KiB threshold produces one
  size finding
- a decoded content prefix above the declared eighty percent threshold produces
  one prefix finding
- JSON syntax and the Locus envelope do not create prefix findings
- clean, finding, and malformed inspections exit zero, one, and two
- every complete inspection reports coverage, including zero analyzer coverage
- malformed declaration or input refuses without a partial report
- role query without a key enumerates sorted identities and record counts
- role query with a key replays matching logical Atoms in input order
- every complete query reports total, matched, and identity coverage
- empty and unmatched query input succeeds with explicit zero coverage
- malformed query input refuses without partial output
- query never requires a product root or analyzer declaration
- format, clippy, tests, Plumb, and Ectropy are green

## Must not require

- span start, end, parent, nesting, tree, status, or duration
- inner-block or expression-level automatic tracing
- an ambient runtime context
- caller-controlled report, flush, retry, or drain
- arbitrary collector injection or default ambient scanning
- whole-environment, whole-argv, or whole-process collection
- HTTP reporting
- persistent query storage, indexing, arbitrary expressions, or aggregation
- query inference of liveness, ownership, lifecycle, causality, or repair
- arbitrary mapping injection, expression languages, automatic threshold tuning
- inspection cause inference or repair advice
- a canonical binary codec
- OpenTelemetry compatibility or dependency

## Commands

```sh
runseal :guard
cargo test --locked --workspace
plumb doctor .
ectropy .
```

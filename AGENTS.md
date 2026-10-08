# Agents

Read the canonical [PerishLab delivery governance](https://github.com/PerishLab/.github/blob/main/GOVERNANCE.md)
at work start and again before delivery or Issue closure. That document owns
organization-wide Issue, pull-request and acceptance policy; this file keeps
repository-specific constraints without copying that policy.

Locus is the language-independent law and first Rust implementation for
readonly context, semantic identity resolution, immutable records, reporting,
and engine diagnostic handoff.

## Closure

- Observation is semantically transparent to the observed product. Removing
  Locus may remove evidence, but never business capability, state, output,
  exit status, or control flow. Observation failures stay visible to the audit
  operator and isolated from the observed command.
- Context is an immutable view. A derived view may shadow one role while every
  prior view keeps its meaning. Complete history is never propagated as
  Context.
- A requested role resolves in this order: explicit key, inherited context key,
  configured generator.
- Only absence falls through. Invalid explicit, inherited, generated, or stored
  keys refuse.
- Callers explicitly select Locus-owned collectors and bind one collected
  observation to a role. Locus never discovers ambient facts on its own.
- Collector chains advance only on absence. Invalid or failed collection
  refuses, and one successful sample is frozen into the accepted Atom.
- One accepted append produces one immutable Atom record. Log is the
  append-only history of those records.
- Every accepted Atom carries a record id, unique per acceptance (engine
  instance and sequence), and the producer name the product declares in its
  Policy. Both are fixed at acceptance so a consumer can deduplicate and
  attribute without a reporter changing the fact.
- Shared-file generation publishes a complete key before it becomes visible;
  concurrent initializers converge without observing a partial stored value.
- Trace and span are semantic roles over records. They imply no parenthood,
  nesting, lifecycle, status, or duration.
- Automatic tracing emits one entry and one normal-return record per traced
  function declaration. Error returns are normal returns; panic, abort,
  cancellation, process exit, and process loss leave the entry without an
  invented terminal fact.
- Reporting starts only after acceptance and cannot change accepted facts.
  Reporter failure never rolls acceptance back.
- Callers select Locus-owned reporters through config. They cannot invoke,
  inject, flush, retry, or drain reporter implementations.
- Reporter declarations are exact. A new reporting capability enters as a new
  reporter name, never as a new option on an existing one, so an older engine
  refuses it at bootstrap through the diagnostic path instead of half-honoring
  it.
- The spool reporter hands Atoms off through `active.jsonl` and immutable,
  time-ordered `sealed-*` segments, with one contract on every platform a
  product ships to. Writers serialize on `spool.lock`, a file never renamed,
  and open `active.jsonl` only for one append, so no writer holds a segment
  that may be sealed and no file identity is needed. The sealed and claimed
  byte total lives in that lock file as an upper bound; a missing or stale
  total is recounted from the directory. Writers never lock a sealed or
  claimed segment, so a consumer never blocks the observed product. A consumer
  claims sealed segments by renaming them to `claimed-*`, then stores and
  removes them; writers never touch a claimed segment.
- The product-declared spool ceiling bounds active, sealed, and claimed bytes
  and is only a safety valve. Past it, the oldest unclaimed sealed segments are
  deleted, their records counted in `loss.jsonl`, and the loss reported to the
  hook as `reporter.loss`. Loss never re-enters the Atom stream.
- Hooks observe immutable outcomes. Their return value cannot influence
  context, acceptance, or reporting.
- Engine diagnostics never re-enter the Atom stream or reporter runtime. They
  hand off to the configured hook, with a terminal stderr adaptor as default.
- JSONL is the cold-start codec, not the permanent logical encoding.
- File reporting preserves one encoded record boundary across concurrent
  engines and processes.
- CLI inspection reads one root `locus.toml`, consumes JSONL from stdin, and
  composes Locus-owned mappings with product-declared thresholds. It emits only
  domain-independent structural findings and an explicit coverage summary.
  Malformed declaration or input refuses without partial output. It never
  changes Atom acceptance or attributes a finding to a cause. The held
  time mapping runs the span derivation, refuses the same crossings, and counts
  the refused spans in its summary; it groups by the entering record's own
  Context and never inherits a key from an enclosing frame.
- CLI query consumes JSONL from stdin, or the same stream from a `locus-api`
  through `--api`, and selects one exact role with an
  optional exact key. It enumerates identities or replays matching logical
  Atoms, then reports complete coverage without inferring lifecycle, ownership,
  causality, or liveness.
- CLI span consumes JSONL from stdin and pairs one entering source record with
  its returning record. It reports elapsed and held time per traced declaration
  and names every entered span that never returned. Held time reads containment
  within one trace as the only nesting evidence, so partial overlap refuses
  rather than attributing a share it cannot support. That refusal is scoped to
  the time the crossing reaches: every span meeting it is named and dropped,
  every span clear of it still derives. Scoping never widens what is claimed,
  because a dropped span always contains any span dropped inside it.
  Recovering the crossing itself would need a parent link the records do not
  carry, so it is never guessed.

## Server

- `locus-api` records faithfully and derives nothing. It keeps every Atom it
  takes over verbatim and returns stored records byte for byte; query, span,
  and inspection stay in the CLI, so `--api` and stdin derive identically.
- Three interfaces are fixed; the internals behind them may change.
  - Handoff: a product's spool is drained by claiming its sealed segments,
    storing them, and removing them. A product still on the file reporter is
    taken over by renaming its report file beside itself, reading taken files
    by offset in complete lines, and removing one only after it has stayed idle
    past the grace period; a writer holding the old descriptor loses nothing
    within the grace. That file form remains until products move to the spool.
    Lines that are not Atoms are kept in `rejected/<producer>.jsonl`, never
    stored.
  - Store: append a batch of one producer's records; read by time window
    `[from, to)` with an optional exact role/key and producer; drop one whole
    producer-day partition. Reads return stored order within a partition and
    partitions in day, then producer, order.
  - Read API: `GET /api/v1/atoms` streams stored records as JSONL.
    `locus-api export-openapi` and its snapshot test own the HTTP contract; the
    `--help` snapshots own both command surfaces.
- Delivery is at least once: a crash between a store append and its offset
  record re-reads those lines.
- The server listens on loopback only and carries no authentication yet.

## Ownership

Locus owns Context and Atom laws, collector and generator algorithms, reporter
execution, config gates, hooks, diagnostic handoff, inspect mappings, query
selection, and threshold mechanics. Products own collector selection and
binding, their event vocabulary, logical cycles, analyzer identities and
thresholds, endpoints and credentials, retention choices, and consumption
policy.

## Feedback

- Let real feedback expose each product's core path. Add the smallest isolated,
  removable observation adapter, exercise the same path, consume its records,
  remove the sharpest supported friction, and repeat until evidence goes flat
  or an ownership decision appears.
- Treat inspection as progressive sieving. Use the coarsest mapping and
  threshold that still catches an unambiguous excess; clear those findings
  before tightening the sieve. Smaller excess can exist without being the next
  economical target.
- Place source tracing only at function declarations through a function-level
  macro. A function is the minimum observation boundary, not a Locus domain
  abstraction. If it is too coarse, split the function at the semantic boundary
  exposed by feedback instead of tracing an inner block; that boundary is a
  structure fact dynamic evidence found beyond Ectropy's static KISS laws.
  Keep function records orthogonal to normalized CLI, agent, service, or other
  product surfaces and join them only through context.
- Preserve atomic facts. Derive duration, nesting, cycles, overlap,
  aggregation, and attribution downstream rather than encoding them at the
  source.
- Treat repeated uncovered mechanisms as public-library candidates, never
  automatic extractions. Promote one only when it repeats across products and
  product names can be removed without changing ownership, refusal, failure,
  or context laws.
- Let an extracted library carry its own observation points so better evidence
  returns to every product. Optimize implementation reuse and agent context as
  one feedback economy without moving product vocabulary into the substrate.

## Layout

- `crates/locus` is the engine and public substrate.
- `crates/macro` is the source adapter and shares the exact release version.
- `crates/cli` is the stdin-first structural inspector and exact Atom query.
- `crates/api` is the `locus-api` server: takeover, store, and the read API.
- `.runseal/hooks` carries the Plumb Guard Git hooks; `runseal.toml` is the Runseal profile.

## Operating

- Never commit directly on `main`.
- Before landing, run `plumb doctor .`, `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo check --locked --workspace --all-targets --release`, `cargo test --locked --workspace`, and `ectropy .`.
- Land only through `plumb land`.

## Release

- Locus is a Cargo-only product. wharf publishes `locus-macro`, `locus`, and
  `locus-cli` to the `perish` registry at `cargo.perish.uk`. It declares no
  binaries and no skill, so it has no target matrix, archive, manager or skill
  generation; its release authority carries the distribution record wharf
  keeps for every marker. The `locus` command reaches an operator through `locus-cli`.
  `locus-api` is not published yet.
- A release follows Plumb's lifecycle (`plumb release --help`); wharf
  publishes the crates in dependency order and reads each one back from the
  index. A rerun publishes only what is missing.
- A stable version owes its changelog on Depot before the next marker is
  stamped.
- Never publish a crate by hand. A crate published outside wharf leaves no
  distribution record.

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
- The `api` reporter is the only reporter. Its buffer hands Atoms off through
  `active.jsonl` and immutable, time-ordered `sealed-*` segments, with one
  contract on every platform a product ships to. Writers serialize on `spool.lock`, a file never renamed,
  and open `active.jsonl` only for one append, so no writer holds a segment
  that may be sealed and no file identity is needed. The sealed and claimed
  byte total lives in that lock file as an upper bound; a missing or stale
  total is recounted from the directory. Writers never lock a sealed or
  claimed segment, so a consumer never blocks the observed product. A consumer
  claims sealed segments by renaming them to `claimed-*`, then stores and
  removes them; writers never touch a claimed segment.
- A product reports to `locus-api` by declaring the `api` reporter with only
  an endpoint and a buffer directory. Locus buffers through the spool contract
  under bounds it owns, sealing a segment by size or once it is a minute old so
  short-lived products become visible, and registers the buffer with the
  endpoint when the engine is built. Registration never blocks or fails the
  product: a buffer that has registered before stays silent while the server is
  down, and one never registered reports `reporter.registration` to the hook.
- The Locus-owned spool ceiling bounds active, sealed, and claimed bytes
  and is only a safety valve. Past it, the oldest unclaimed sealed segments are
  deleted, their records counted in `loss.jsonl`, and the loss reported to the
  hook as `reporter.loss`. Loss never re-enters the Atom stream.
- Hooks observe immutable outcomes. Their return value cannot influence
  context, acceptance, or reporting.
- Engine diagnostics never re-enter the Atom stream or reporter runtime. They
  hand off to the configured hook, with a terminal stderr adaptor as default.
- JSONL is the cold-start codec, not the permanent logical encoding.
- The buffer preserves one encoded record boundary across concurrent engines
  and processes.
- The CLI reads Atoms only from a `locus-api` read API, at `--api` (default
  `http://127.0.0.1:43308`), narrowed by its window, selector and producer.
  When the server reports a retained boundary, every summary carries it as
  `retained`, so a headless return can be read against it.
- CLI inspection reads one root `locus.toml`, consumes that stream, and
  composes Locus-owned mappings with product-declared thresholds. It emits only
  domain-independent structural findings and an explicit coverage summary.
  Malformed declaration or input refuses without partial output. It never
  changes Atom acceptance or attributes a finding to a cause. The held
  time mapping runs the span derivation, refuses the same crossings, and counts
  the refused spans in its summary; it groups by the entering record's own
  Context and never inherits a key from an enclosing frame.
- CLI query selects one exact role with an optional exact key. It enumerates identities or replays matching logical
  Atoms, then reports complete coverage without inferring lifecycle, ownership,
  causality, or liveness.
- CLI span pairs one entering source record with
  its returning record. It reports elapsed and held time per traced declaration
  and names every entered span that never returned. Held time reads containment
  within one trace as the only nesting evidence, so partial overlap refuses
  rather than attributing a share it cannot support. That refusal is scoped to
  the time the crossing reaches: every span meeting it is named and dropped,
  every span clear of it still derives. Scoping never widens what is claimed,
  because a dropped span always contains any span dropped inside it.
  Recovering the crossing itself would need a parent link the records do not
  carry, so it is never guessed.
- A return whose entry is not in the stream is named `headless` and counted,
  never measured and never refused as a whole input. The read window and the
  spool's designed loss both produce one, and neither side knows where a shed
  gap lies, so its cause is never attributed. Its interval runs from the head
  of the stream to its return, ahead of any complete span starting there: a
  complete span in the same trace that strictly contains the return cannot be
  judged nested or crossing and is refused under the scoped rule, while spans
  that end by the return still derive. The held time mapping follows the same
  rule and counts headless returns in its summary.

## Server

- `locus-api` records faithfully and derives nothing. It keeps every Atom it
  drains verbatim and returns stored records byte for byte; query, span, and
  inspection stay in the CLI.
- Three interfaces are fixed; the internals behind them may change.
  - Handoff: a product's buffer registers itself through
    `POST /api/v1/spools` (a directory holding `spool.lock`), and the server
    persists registrations in `spools.json` under its home. Registration is
    the only way a buffer reaches the server. A registered buffer is drained
    by claiming its sealed segments, reading each by offset in complete lines,
    storing them, and removing them. Lines that are not Atoms are kept in
    `rejected/<producer>.jsonl`, never stored.
  - Store: append a batch of one producer's records under a batch token;
    read by time window `[from, to)` with an optional exact role/key and
    producer; drop every producer-day partition before a day; tick, which
    seals and sweeps whatever the store keeps open. Within one trace, reads
    return records in time order.
  - Read API: `GET /api/v1/atoms` streams stored records as JSONL. With a
    declared retention it names, in `locus-retained-from`, the instant before
    which history is not retained.
    `locus-api export-openapi` and its snapshot test own the HTTP contract; the
    `--help` snapshots own both command surfaces.
- Delivery is at least once: a crash between a store append and its offset
  record re-reads those lines. The drained segment and offset are the batch
  token, so the store applies a re-read batch once.
- The store is `keel-column` (`Columns`): raw Atom bytes verbatim, `at` as
  time, producer and `locus.trace` as key columns sorted by trace, partitioned
  by producer and day. Its manifest lives in a Keel estate the server hosts at
  `<home>/estate.sqlite3`, bootstrapped on first start, with parts under
  `<home>/column`. Producer and trace selections prune parts; any other role
  is selected exactly by reading each candidate. A JSONL `store` directory
  left by an earlier server is migrated at startup, one day at a time, and a
  day file is removed only after it reads back byte for byte.
- The server listens on loopback only and carries no authentication yet.
- Retention is declared only in `[retention] days` of `<home>/locus-api.toml`,
  the server's standard config file; malformed config refuses at startup.
  Undeclared, nothing is deleted. Declared, each drain cycle drops every
  producer-day partition wholly before the current day minus `days`, through
  the Store interface, so the rule holds whatever store sits behind it.

## Ownership

Locus owns Context and Atom laws, collector and generator algorithms, reporter
execution and buffer bounds, config gates, hooks, diagnostic handoff, inspect
mappings, query selection, and threshold mechanics. Products own collector
selection and binding, their event vocabulary, logical cycles, analyzer
identities and thresholds, the endpoint they report to and the buffer
directory under their own state, and consumption policy. `locus-api` owns
where records go and how long they stay. Locus is a library: it reads neither
configuration files nor the environment; a product reads its own standard
configuration file and hands the result to `Policy`.

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
- `crates/cli` is the structural inspector and exact Atom query over the read API.
- `crates/api` is the `locus-api` server: drain, column store and migration, registry, retention, and the read API.
- `packaging/deb` is the `locus-api` Debian placement: control, maintainer scripts, and `root/` payload.
- Git hooks are not tracked: `plumb configuration install` writes the Plumb Guard hooks into Git's default hooks path. `runseal.toml` is the Runseal profile.

## Operating

- Never commit directly on `main`.
- Before landing, run `plumb doctor .`, `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo check --locked --workspace --all-targets --release`, `cargo test --locked --workspace`, and `ectropy .`.
- Land only through `plumb land`.

## Release

- wharf publishes `locus-macro`, `locus`, and `locus-cli` to the `perish`
  registry at `cargo.perish.uk`; the `locus` command reaches an operator
  through `locus-cli`. Locus declares no skill.
- `locus-api` is the one declared binary, built for `x86_64-unknown-linux-gnu`
  with `install = false`, and ships only as the Debian placement in
  `packaging/deb` (`linux-x64-deb` in the seal; wharf keeps no apt
  repository). The package carries `locus-api@.service`, which runs the
  zero-flag server as `User=%i` with its home at that user's `.locus`, so it
  reads the Locus buffers that user's products registered with that user's
  permissions. Its maintainer scripts only reload systemd: they never enable,
  start, restart, or delete data. An operator enables it per user with
  `systemctl enable --now locus-api@<user>`.
- A release follows Plumb's lifecycle (`plumb release --help`); wharf
  publishes the crates in dependency order and reads each one back from the
  index. A rerun publishes only what is missing.
- A stable version owes its changelog on Depot before the next marker is
  stamped.
- Never publish a crate by hand. A crate published outside wharf leaves no
  distribution record.

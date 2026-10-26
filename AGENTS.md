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
- Context is an immutable view. A derived view may shadow one role, and a
  fork view replaces the trace and span with a fresh trace and its parent link,
  while every prior view keeps its meaning. Complete history is never
  propagated as Context.
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
- Trace and span are semantic roles over records. They imply no lifecycle,
  status, or duration. Nesting has exactly two kinds of evidence: containment
  within one trace, and an explicit parent link. Neither is ever inferred from
  time alone across traces.
- `locus.parent` is the parent link: a role whose key names the span that
  launched a trace. It is a recorded fact about one trace, bound on any record
  of that trace, and need not be inherited by every record. Across processes
  it enters only through the Locus-owned `LOCUS_PARENT` variable and its
  `environment` collector; writing that variable into a child's environment is
  the launching product's act, and Locus itself still reads no environment.
  Within a process, `Engine::fork` derives from a frame Context a view with a
  fresh trace, the frame's span as parent, and no span. `trace` exposes its
  frame Context to the function body only through an explicit `frame` name.
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
  claimed segment, so storing claimed records never blocks the observed product.
  Before claiming, a consumer may seal an aged active segment under a
  nonblocking attempt on the publication lock; a busy writer postpones sealing
  to the next cycle. This lock is released before records are read or stored.
  A consumer claims sealed segments by renaming them to `claimed-*`, then stores and
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
- Derivation (query selection, tree expansion, span pairing, held time and crossing refusal,
  inspect mappings, thresholds, coverage and summaries) lives in the
  `locus-derivation` library, so any reader of Atoms answers with the same
  law. It observes one Atom at a time and finishes into a report.
  - Its input is one ordered, bounded sequence: within each span the entry
    precedes the return, and refusal, headless returns and coverage are
    defined over exactly that sequence, the read window. Records are counted
    as observed.
  - It assumes no representation. The `representation-bytes` mapping measures
    the encoded size the caller passes with each Atom; the CLI passes the
    JSONL line as read.
  - A refusal is an error from observe; the caller then emits nothing.
    Query by role and key holds its matching Atoms until the sequence ends,
    because no partial output may precede a refusal.
- The CLI is an adapter: it reads Atoms only from a `locus-api` read API, at `--api` (default
  `http://127.0.0.1:43308`), narrowed by its window, selector and producer.
  It sends `--token` (or `LOCUS_API_TOKEN`) as a Bearer token, never observed.
  When the server reports a retained boundary, every summary carries it as
  `retained`, so a headless return can be read against it. Runs go through
  `plumb::observation`; a refused read records its API fault code on `locus.fault`.
  SIGINT and SIGTERM stop a run cooperatively: the scan refuses before its
  next record, every traced function returns, and the run finishes and exits
  with 130 or 143. A read still blocked two seconds later exits with the same
  code and leaves its spans open, as a kill does.
- Crates that observe themselves through `plumb::observation` (locus-cli, locus-api)
  take `locus` from the published registry `"0"`, the same copy plumb carries.
  `locus-derivation` also consumes that published Atom contract and never depends
  on plumb; the CLI consumes workspace derivation so its adapters and new
  derivations are proved together without a locus -> plumb -> locus cycle.
  No `[patch]` unifies the workspace engine with plumb's.
- Inspection runs Locus's default analyzers unless a product turns them off:
  `trace.size`, `trace.prefix`, `trace.held`, `span.size`, `span.prefix` and
  `span.held`, each with a coarse threshold that only an unambiguous excess
  crosses. What to inspect is Locus's; how strictly is the product's.
- CLI inspection reads an optional root `locus.toml` and consumes that stream.
  A product redeclares a default analyzer with the same mapping to set its
  threshold, lists it under `disable` to turn it off, and declares new
  analyzers after the defaults; a redeclaration with another mapping, or one
  that is also disabled, refuses. Without the file the defaults run as they
  are. The summary names each default's origin. It emits only
  domain-independent structural findings and an explicit coverage summary.
  Malformed declaration or input refuses without partial output. It never
  changes Atom acceptance or attributes a finding to a cause. The held
  time mapping runs the span derivation, refuses the same crossings, and counts
  the refused spans once in its summary, however many held analyzers run; it
  groups by the entering record's own Context and never inherits a key from an
  enclosing frame.
- `locus analyze [root]` reads the API once and captures at most `--max-bytes`
  (default and ceiling 512 MiB) in memory. It refuses more than `--max-records`
  (default and ceiling one million), a line over 1 MiB, a missing final API
  newline, or more than 64 effective analyzers. These bound input, not peak RSS.
  It feeds the identical sequence to inspection and span, then replays that
  capture to the derivation library's finding evidence projection. Nothing is
  emitted until capture, parsing and all derivations succeed. Existing commands
  retain their contracts. No local-file input or server snapshot is added.
- Analyze emits existing inspect/span JSONL records, evidence groups keyed by
  finding role/key, and a final `locus.analyze/v1` summary with request bounds
  and filters, retained boundary, version, byte/record counts, exact input
  SHA-256, effective configuration and limits. Tokens are omitted. The summary
  seals only this read, never an ingestion watermark or complete history;
  a missing final summary means incomplete output. Exit codes are 0 clean,
  1 findings and 2 refused, with signal exits as above.
- Finding evidence reports exact matching record/encoded-byte totals, up to four
  first-seen metadata references (ordinal, optional id, time, producer, context,
  source) and up to 32 first-seen producer/source contribution counters per
  group. It counts omitted references and records outside retained contributions.
  Counts are not function duration, prefix-matching attribution or causes.
  Null origins remain unknown; contexts are samples, never a unique command or
  invocation assignment. At most 10,000 groups and 16 MiB of serialized identity,
  reference and contribution content are admitted globally; identity overflow
  refuses while content overflow increments omissions. Counters and envelopes
  add bounded overhead. No payload is copied into evidence. Input is released
  on exit and no temporary files are created.
- CLI query selects one exact role with an optional exact key. It enumerates identities or replays matching logical
  Atoms, then reports complete coverage without inferring lifecycle, ownership,
  causality, or liveness.
- CLI tree reads one API window as a stream and selects seed Atoms by exact
  conditions: `context.ROLE=KEY` or a top-level `payload.FIELD=VALUE`, where a
  string matches equal text and a number or boolean matches its JSON text.
  Repeated conditions all hold. It expands each seed only through the two kinds
  of nesting evidence: containment within one trace, and a `locus.parent` link
  that resolves to the one other trace binding that span. An unobserved parent,
  a parent span bound in several traces, or a trace binding two parents leaves
  that trace unlinked. These links are counted per tree as missing, ambiguous
  or conflicting, never guessed, and a root carrying one is detached. A parent
  cycle roots at its least trace key. Each tree reports its root, traces,
  records, time bounds and links, then its seed Atoms and the Atoms matching
  separate show conditions, verbatim and with multiplicity. Atom id
  representation is never read as invocation identity. The final
  `locus.tree/v1` summary carries the request, conditions, bounds, `retained`
  and the window incompleteness marks used by span. It holds only trace/span
  keys and matching Atoms, never raw input. Explicit limits on trees, held
  Atoms and tracked keys refuse without partial output. A tree claims what the
  window holds, not lifecycle, success, causality or complete history.
- CLI span pairs one entering source record with
  its returning record. It reports elapsed and held time per traced declaration
  and names every entered span that never returned. Held time reads
  containment within one trace and explicit parent links as its only nesting
  evidence, so partial overlap within a trace refuses rather than attributing a
  share it cannot support. That refusal is scoped to
  the time the crossing reaches: every span meeting it is named and dropped,
  every span clear of it still derives. Scoping never widens what is claimed,
  because a dropped span always contains any span dropped inside it.
  A crossing inside one trace is never recovered by guessing a parent.
- A parent link nests a whole linked trace under one complete span of another
  trace. When every interval of the linked trace lies within that span, the
  span's held time subtracts the union of its direct children and its linked
  traces' intervals, so concurrent linked traces never cross each other. A
  linked trace reaching outside its parent refuses under the scoped rule in the
  parent's trace, while the linked trace still derives on its own. A link whose
  parent is not a complete span in the stream, or a trace carrying two
  different parents, derives standalone and is counted. Span and inspection
  summaries report `links` as nested, outlived, missing and conflicting counts.
- A return whose entry is not in the stream is named `headless` and counted,
  never measured and never refused as a whole input. The read window and the
  spool's designed loss both produce one, and neither side knows where a shed
  gap lies, so its cause is never attributed. Its interval runs from the head
  of the stream to its return, ahead of any complete span starting there: a
  complete span in the same trace that strictly contains the return cannot be
  judged nested or crossing and is refused under the scoped rule, while spans
  that end by the return still derive. The held time mapping follows the same
  rule and counts headless returns in its summary.
- CLI `--from` and `--to` take nanoseconds since epoch or an RFC3339 instant
  with `Z` or an explicit offset, such as `2026-10-17T21:00:00+08:00`; a
  zoneless or malformed instant refuses rather than guessing a zone.
- The span summary adds `incomplete` only when a read boundary can explain its
  anomalies, listing each that can: `window.from` or `retention` for headless
  returns or missing parents when `--from` or the retained start cuts the
  stream, `window.to` for unclosed spans or missing parents under `--to`, and
  `spool` when the window reaches the last 75 seconds, where a return may wait
  in an unsealed spool for the 60 second seal and a drain. It names possible
  boundaries, never an attributed cause: widen the window or wait and reread
  before treating those counts as defects. A summary without anomalies omits it.

## Server

- `locus-api` records faithfully and derives nothing. It keeps every Atom it
  drains verbatim and returns stored records byte for byte; query, span, and
  inspection derive downstream in `locus-derivation`.
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
    which history is not retained. Guarded, it answers 401 with
    `WWW-Authenticate: Bearer` to a request without a declared reader's token.
    `locus-api export-openapi` and its snapshot test own the HTTP contract; the
    `--help` snapshots own both command surfaces.
- Delivery is at least once: a crash between a store append and its offset
  record re-reads those lines. The drained segment and offset are the batch
  token, so the store applies a re-read batch once.
- The store is `keel-column` (`Columns`): raw Atom bytes verbatim, `at` as
  time, producer and `locus.trace` as key columns sorted by trace, partitioned
  by producer and day. Its manifest lives in a Keel estate the server hosts at
  `<home>/estate.sqlite3`, bootstrapped on first start, with parts under
  `<home>/column`. The server explicitly adopts its original field-free column
  declaration through `Store::adopt`: a nonempty root without `model.json`
  must validate against that declaration before binding and recovery. An
  existing binding must match exactly; invalid models, incompatible layouts
  and malformed unbound history refuse without rewriting historical record
  files. No nullable fields are added or inferred. Old writers do not enforce
  bindings, so rollback across adoption requires a compatibility review.
  Producer and trace selections prune parts; any other role
  is selected exactly by reading each candidate. A JSONL `store` directory
  left by an earlier server is migrated at startup, one day at a time, and a
  day file is removed only after it reads back byte for byte.
- The server listens on loopback only. Read access is guarded by digest
  bearer: each `[[reader]]` in `<home>/locus-api.toml` names one reader and
  holds only the SHA-256 digest of its token, never the token. With readers
  declared, `GET /api/v1/atoms` admits only a Bearer token whose digest
  matches one; without them it stays open. `/health` and `POST /api/v1/spools`
  are never guarded, so registration never blocks a product. Names and digests
  are unique; a malformed reader refuses at startup and fails doctor.
- `locus-api` observes itself through Locus, never through tracing
  subscribers. The `[locus] enabled` switch in `config.toml` under the
  platform `.locus` home, the same gate `export-openapi` reads and separate
  from `locus-api.toml`, turns it on; off is the default. `serve` then records
  startup migrate and each drain, seal and retention as traced spans with
  their countable facts (drain sources, segments, records and rejected lines;
  migrated days; the declared days and retain floor; each step's outcome) into
  `<home>/state/locus-api`, an `api` buffer whose endpoint is never a server.
  That buffer is never registered, so the server's own records never pass its
  `POST /api/v1/spools` or drain, whatever endpoint `[locus]` names. Observation
  failures reach stderr and never change a cycle's result or stop `serve`.
- Retention is declared only in `[retention] days` of `<home>/locus-api.toml`,
  the server's standard config file, which also holds `[[reader]]`; malformed config refuses at startup.
  Undeclared, nothing is deleted. Declared, each drain cycle drops every
  producer-day partition wholly before the current day minus `days`, through
  the Store interface, so the rule holds whatever store sits behind it.
- `doctor` names silent producers but never changes their gates. Each product
  still switches its own observation on through its own `[locus]` file, and a
  missing file stays silent there. The `producers` check lists every registered
  producer with the latest modification time of the files that producer writes
  into its buffer (`registration`, `active.jsonl`, `sealed-*`, `claimed-*`;
  never `spool.lock`). It warns when a producer named in the optional
  `[producers] expected` list of `locus-api.toml` never registered, or when a
  registered producer wrote nothing for 24 hours. `serve` ignores that list.

## Ownership

Locus owns Context and Atom laws, collector and generator algorithms, reporter
execution and buffer bounds, config gates, hooks, diagnostic handoff, inspect
mappings, the default analyzers and their thresholds, query selection, and
threshold mechanics. Products own collector selection and binding, their
event vocabulary, logical cycles, threshold overrides, disabled defaults and
added analyzers, the endpoint they report to and the buffer
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
- `crates/derivation` is the derivation library over an ordered Atom sequence.
- `crates/cli` is the `locus` command: it reads the read API, decodes JSONL and renders `crates/derivation` reports.
- `crates/api` is the `locus-api` server: drain, column store and migration, registry, retention, and the read API.
- `packaging/deb` is the `locus-api` Debian placement: control, maintainer scripts, and `root/` payload.
- Git hooks are not tracked: `plumb configuration install` writes the Plumb Guard hooks into Git's default hooks path. `runseal.toml` is the Runseal profile.

## Operating

- Never commit directly on `main`.
- Before landing, run `plumb doctor .`, `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo check --locked --workspace --all-targets --release`, `cargo test --locked --workspace`, and `ectropy .`.
- Land only through `plumb land`.

## Release

- wharf publishes `locus-macro`, `locus`, `locus-derivation`, and `locus-cli`
  to the `perish` registry at `cargo.perish.uk`; the `locus` command reaches
  an operator through `locus-cli`. Locus declares no skill.
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

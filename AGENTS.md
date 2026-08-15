# Agents

Locus is the language-independent law and first Rust implementation for
readonly context, semantic identity resolution, immutable records, reporting,
and engine diagnostic handoff.

## Closure

- Observation is semantically transparent to the observed product. Removing
  Locus may remove evidence, but never business capability, state, output,
  exit status, or control flow.
- Context is an immutable view. A derived view may shadow one role while every
  prior view keeps its meaning.
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
- Shared-file generation publishes a complete key before it becomes visible;
  concurrent initializers converge without observing a partial stored value.
- Trace and span are semantic roles over records, not lifecycle containers.
- Reporting starts only after acceptance and cannot change accepted facts.
- Callers select Locus-owned reporters through config. They cannot invoke,
  inject, flush, retry, or drain reporter implementations.
- Hooks observe immutable outcomes. Their return value cannot influence
  context, acceptance, or reporting.
- Engine diagnostics never re-enter the Atom stream or reporter runtime. They
  hand off to the configured hook, with a terminal stderr adaptor as default.
- JSONL is the cold-start codec, not the permanent logical encoding.
- File reporting preserves one encoded record boundary across concurrent
  engines and processes.
- CLI inspection reads one root `locus.toml`, consumes JSONL from stdin, and
  composes Locus-owned mappings with product-declared thresholds. It emits only
  domain-independent structural findings and an explicit coverage summary. It
  never changes Atom acceptance or attributes a finding to a cause. The held
  time mapping runs the span derivation, refuses the same crossings, and counts
  the refused spans in its summary; it groups by the entering record's own
  Context and never inherits a key from an enclosing frame.
- CLI query consumes JSONL from stdin and selects one exact role with an
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
- `skills/locus` carries the temporary transparent-instrumentation convention.
- `DESIGN.md` is the current semantic doctrine for observation and identity.
- `.runseal` and `.forgejo` are thin workshop operator surfaces.

## Operating

- Never commit directly on `main`.
- Before landing, run `plumb doctor .`, `cargo fmt --all --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo check --locked --workspace --all-targets --release`, `cargo test --locked --workspace`, and `ectropy .`.
- Land only through `plumb land`.

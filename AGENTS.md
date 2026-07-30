# Agents

Locus is the language-independent law and first Rust implementation for
readonly context, semantic identity resolution, immutable records, reporting,
and engine diagnostic handoff.

## Closure

- Context is an immutable view. A derived view may shadow one role while every
  prior view keeps its meaning.
- A requested role resolves in this order: explicit key, inherited context key,
  configured generator.
- Only absence falls through. Invalid explicit, inherited, generated, or stored
  keys refuse.
- One accepted append produces one immutable Atom record. Log is the
  append-only history of those records.
- Trace and span are semantic roles over records, not lifecycle containers.
- Reporting starts only after acceptance and cannot change accepted facts.
- Callers select Locus-owned reporters through config. They cannot invoke,
  inject, flush, retry, or drain reporter implementations.
- Hooks observe immutable outcomes. Their return value cannot influence
  context, acceptance, or reporting.
- Engine diagnostics never re-enter the Atom stream or reporter runtime. They
  hand off to the configured hook, with a terminal stderr adaptor as default.
- JSONL is the cold-start codec, not the permanent logical encoding.

## Ownership

Locus owns Context and Atom laws, key resolution and generator algorithms,
reporter execution, config gates, hooks, and diagnostic handoff. Products own
their event vocabulary, logical cycles, endpoints and credentials, retention
choices, and consumption policy.

## Layout

- `crates/locus` is the engine and public substrate.
- `crates/macro` is the source adapter and shares the exact release version.
- `docs/model/laws.md` is the prose wall for semantic laws.
- `docs/run/verify.md` is the cold-start verification boundary.
- `.runseal` and `.forgejo` are thin workshop operator surfaces.

## Operating

- Never commit directly on `main`.
- Run `runseal :init` after clone.
- Run `runseal :guard` before landing.
- Land only through `runseal :land`.
- Publish the coupled Cargo packages only through `runseal :release`.

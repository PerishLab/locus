---
name: locus
description: Operate Locus's readonly context and immutable observation substrate. Use when instrumenting a Rust product with transparent trace/span observations; selecting collectors, generators, reporters, or hooks; querying or inspecting Atom JSONL with the Locus CLI; diagnosing observation coverage or delivery gaps; or changing the Locus repository, locus.toml, semantic laws, vocabulary, or delivery conventions.
---

# Locus

Treat Locus as a faithful historian. Observed products produce facts; Locus
accepts, relates, reports, and queries them without gaining authority over the
products' business delivery.

## Core boundary

- Preserve semantic transparency. Removing or disabling Locus may remove
  evidence, but never business capability, state, output, exit status, or
  control flow.
- Keep Locus Context inside the observation path. Never use a trace, span,
  collection, report outcome, or query result to authorize or select business
  behavior.
- Keep product vocabulary opaque to Locus. Bind exact product-owned facts to
  roles without adding product names to Locus algorithms.
- Preserve atomic facts. Derive duration, nesting, overlap, aggregation,
  attribution, lifecycle, and liveness downstream.
- Keep observation failure visible to the audit operator and isolated from the
  observed command's result.

## Add a temporary product adapter

1. Discover the product's repository instructions and normal verification.
2. Create or reuse one isolated observation module and one readonly process
   seat. Do not expose Locus types through business APIs or state models.
3. Gate bootstrap before collection, generation, or reporter setup. Default the
   gate to muted and let an external operator choose the report endpoint.
4. Select exact bounded Locus-owned collectors. Never scan the whole
   environment, argv, process, or filesystem.
5. Attach source tracing only to function declarations through
   `#[locus::trace(with = ...)]`. Split a function when the declaration is too
   coarse; do not trace inner expressions or blocks.
6. Ignore observation outcomes on the business path. Test enabled and disabled
   execution for equivalent business state, output, and exit status.
7. Keep the adapter concentrated and removable. Treat source annotations and
   bootstrap code as temporary syntax intrusion until a simpler external
   instrumentation delivery exists.

## Query and inspect history

Consume explicit Atom JSONL through stdin:

```sh
locus query locus.trace < atoms.jsonl
locus query locus.trace TRACE_KEY < atoms.jsonl
locus query locus.span < atoms.jsonl
locus inspect /path/to/product < atoms.jsonl
```

Use `query ROLE` to enumerate sorted identities with record counts. Add an
exact key to replay matching logical Atoms in input order. Read the final
`locus.query/v1` summary for total, matched, and identity coverage. Empty or
unmatched input is a successful query with explicit zero coverage; malformed
input refuses without partial output.

Use `inspect ROOT` only when the product root carries `locus.toml`. Inspection
composes Locus-owned mappings with product-declared thresholds and emits
measurements, not cause or repair advice.

Do not interpret either command as process liveness, executor ownership,
parenthood, lifecycle, or business authority.

## Change Locus itself

1. Enter the Locus repository and read `AGENTS.md`, `docs/model/laws.md`,
   `docs/model/vocabulary.md`, and `docs/run/verify.md` as required by the
   change.
2. Use the Plumb skill and run `plumb doctor .` before changing repository
   shape. Use the Ectropy skill, read `ectropy.toml`, and run `ectropy .`.
3. Put Context, Atom, collector, generator, reporter, hook, and diagnostic
   mechanics in `crates/locus`; source adaptation in `crates/macro`; stdin-first
   consumption in `crates/cli`.
4. Write the law before or with its mechanism. Keep product analyzer identities,
   thresholds, endpoints, credentials, retention, and consumption policy out
   of the substrate.
5. Run the repository's complete guard before landing.

## Standing

Locus currently mechanizes immutable Context derivation, identity precedence,
exact bounded collectors, generated and shared keys, immutable Atom records,
function declaration tracing, file reporting, diagnostic isolation, structural
inspection, and exact role query with complete summaries.

The skill holds the still-prose obligation that downstream integration remain
semantically transparent and removable. No checker proves every product has
kept Locus out of its business delivery.

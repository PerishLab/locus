---
name: locus
description: Operate Locus's readonly context and immutable observation substrate. Use when instrumenting a Rust product with transparent trace and span observations, selecting collectors, generators, reporters, or hooks, querying Atom JSONL with the Locus CLI, diagnosing observation coverage, or changing the Locus repository.
---

# Locus

Treat Locus as a faithful historian. Observed products produce facts; Locus
accepts, relates, reports, and queries them without gaining authority over the
products' business delivery. Outside an observed product or the Locus
repository this brief is silent.

## Objects

- A **Context** is an immutable readonly view; deriving one shadows a role
  without changing the old view.
- An **Atom** is one immutable accepted record; derived quantities live
  downstream.
- A **role** names one bound fact, such as trace or span.
- A **collector** reads one exact bounded source and a **generator** supplies an
  absent role; a **reporter** delivers accepted records and a **hook** receives
  diagnostics.
- An **adapter** is temporary product-side syntax, deleted once its report is
  consumed.

## Actions

```bash
locus query locus.trace < atoms.jsonl
locus query locus.trace TRACE_KEY < atoms.jsonl
locus inspect /path/to/product < atoms.jsonl
```

The binary is the authority for flags; prefer `locus --help`.

## Operating laws

- Preserve semantic transparency. Removing or disabling Locus may remove
  evidence, but never business capability, state, output, exit status, or
  control flow.
- Keep Context inside the observation path. Never let a trace, span, collection,
  report outcome, or query result authorize or select business behavior.
- Keep product vocabulary opaque. Bind exact product-owned facts to roles
  without adding product names to Locus algorithms.
- Preserve atomic facts. Derive duration, nesting, overlap, aggregation,
  attribution, lifecycle, and liveness downstream.
- Keep observation failure visible to the audit operator and isolated from the
  observed command's result.
- Never read a whole environment, argv, process, or filesystem; collectors take
  exact bounded selectors and chains advance only on absence.
- Context derivation, identity precedence, bounded collectors, generated and
  shared keys, immutable records, declaration tracing, file reporting,
  diagnostic isolation, inspection, and role query are mechanized. That
  downstream integration stays transparent and removable is not; no checker
  proves it.
- Repository shape, dependency policy, and landing belong to Plumb; structural
  and vocabulary law to Ectropy. Never claim either as a Locus check.

Use [PATHS.md](PATHS.md) for routine flows and [SCENARIOS.md](SCENARIOS.md) only
when one of its bounded cases applies.

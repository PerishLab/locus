# Design

Locus is a fact authority for readonly context and immutable observation. An
accepted candidate becomes an Atom; reporting, query, and inspection cannot
change that fact or the observed product's behavior.

## Transparency

Removing Locus may remove evidence, never business capability, state, output,
exit status, or control flow. Observation failures remain visible to the audit
operator and isolated from the observed command. Hooks see immutable outcomes
and have no authority.

Context is a readonly role-to-key view. Derivation creates another view and may
shadow one role without changing earlier views or records. Complete history is
never propagated as Context.

## Identity

Each requested role resolves independently:

```text
explicit key > inherited context key > configured generator
```

Only absence falls through. Invalid values refuse where they occur and are not
repaired by a lower layer. Trace and span are roles over records; they imply no
parenthood, nesting, lifecycle, status, or duration.

Collectors are Locus-owned bounded algorithms explicitly selected by callers.
One chain advances only on absence. Invalid output or execution failure refuses
and reaches the diagnostic hook. A successful sample becomes explicit identity
input and its provenance is frozen into the Atom. Locus never scans an ambient
environment, argv, process, or filesystem.

A shared-file generator publishes one complete key before its path becomes
visible. Concurrent initializers converge without observing partial state.

## Records and source

One append creates one logical immutable record. JSONL framing, storage,
compression, indexing, and aggregation are representations rather than logical
law. The file reporter preserves record boundaries across processes and creates
private files on Unix. Reporter failure cannot roll acceptance back.

Automatic tracing attaches only to function declarations. It emits entry and
normal return records under caller-managed Context. Error returns are normal;
panic, abort, cancellation, process exit, and process loss leave entry without
inventing a terminal fact. If a function is too coarse, split it at the
semantic boundary rather than tracing an expression or block.

## Query and inspection

Query is a readonly projection over a complete Atom stream. One exact role
enumerates sorted identities; an optional exact key replays matching Atoms in
input order. Every successful query reports total, matched, and identity
coverage. It infers no liveness, ownership, lifecycle, causality, or repair.

Span derivation pairs one entering source record with its returning record and
reports elapsed and held time per traced declaration. An entered span that never
returned is named rather than dropped, because panic, abort, cancellation, and
process loss are exactly the cases worth reading. Held time treats containment
within one trace as the only nesting evidence there is; spans that overlap
without nesting refuse instead of dividing a duration the records cannot
apportion.

That refusal is scoped to the time the crossing reaches rather than to the whole
input. A trace is a semantic role, not a process, so concurrent work under one
trace interleaves by construction and a single crossing would otherwise discard
every unrelated reading in the file. The scope is honest because containment
makes it closed: any span enclosing a dropped span meets the same crossing and
is dropped with it, so a surviving span never subtracts a child that was refused.
Recovering the crossing itself would take a parent link the records do not
carry, and guessing one would be the attribution this derivation refuses.

Inspection reads one product `locus.toml` and composes a closed Locus mapping
with a product-owned analyzer identity and threshold. Findings are structural
measurements, not attribution. The mappings measure encoded bytes, dominant
decoded content prefix, and held time by Context group. Empty coverage is
explicit; malformed declaration or input refuses without partial output.

Held time runs the span derivation rather than a second one, so a mapping and
the `span` command refuse the same crossings and drop the same spans; the
summary counts what was refused so a threshold is never read against a silently
shortened total. A group key comes from the entering record's own Context and is
never inherited from an enclosing frame, because reaching outward for a key
would be the attribution inspection does not make. A product that binds no role
onto its source records therefore groups only by trace, which is a statement
about that product's Context, not a gap in the mapping.

## Ownership

Locus owns Context and Atom laws, collectors, generators, reporters, hooks,
diagnostics, mappings, query selection, and threshold mechanics. Products own
collector bindings, event vocabulary, logical cycles, analyzer identities and
thresholds, endpoints, credentials, retention, and consumption policy.

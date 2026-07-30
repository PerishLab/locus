# Locus laws

## Fact authority

Locus accepts candidate facts and emits immutable Atom records. Acceptance
means the record became a fact. It does not claim persistence or delivery.
Reporter outcomes are separate observations and cannot roll acceptance back.

## Context

Context is a readonly role-to-key view. Derivation creates another view. A new
view may carry a different effective key for one role, while the original view
and every record retain their prior meaning.

Complete Log history is never the propagated Context. Records may retain a
context snapshot, while propagation stays bounded to the effective role view.

## Identity

Each requested role resolves independently:

```text
explicit key > inherited context key > configured generator
```

Only absence falls through. Invalid values refuse where they appear and are
never repaired by a lower layer.

Trace and span are the first stable roles. They do not imply parenthood,
nesting, start, end, status, duration, or a runtime scope.

## Collection

Collectors are Locus-owned readonly observation algorithms. Config names an
ordered chain of built-ins, and a Candidate explicitly binds that chain to one
role. Locus never scans ambient state or chooses what an observation means.

Bootstrap validates collector specs but does not sample. Append evaluates one
requested chain once. Only absence advances to the next collector; invalid
output or execution failure refuses and hands a diagnostic to the hook.

A collected value enters identity resolution as an explicit candidate.
Existing literal explicit values therefore win without running a collector.
Collection provenance is frozen into the Atom without duplicating its value.

The first built-ins select one environment variable, one argv position after
the executable, or one process field. Each spec carries an explicit byte bound.
There is no whole-environment, whole-argv, or whole-process collector.

## Records

One accepted append creates one logical record with a stable boundary. The
record stream is JSONL-like: appendable, streamable, and replayable. UTF-8
JSON, newline framing, storage layout, compression, indexing, aggregation, and
query are not part of the logical law.

## Reporting

Locus owns reporter implementations and execution. Config selects an owned
reporter and gates its use. A caller cannot invoke reporting, supply an
arbitrary reporter, flush, retry, drain, or control one record's delivery.

Hooks observe immutable outcomes after the decision. Their return value has no
authority.

## Diagnostics

Engine diagnostics are not business records. They bypass Atom reporting and
hand off once to the configured hook. The default adaptor writes directly to
stderr. A hook failure is isolated and cannot recurse into reporting.

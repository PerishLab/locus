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

A shared-file generator publishes one complete stored key before the path
becomes visible. Concurrent generators either publish that first key or read
it; they never treat another generator's partial write as stored context.

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

The cold-start file reporter preserves one record boundary across concurrent
engines and processes. Concurrency cannot interleave the encoded bytes of two
accepted Atoms.

## Source

Automatic source tracing attaches only to a function declaration. It records
one enter Atom and one Atom when the function returns normally. A missing
return remains an observable absence after panic, abort, cancellation, or
process loss; the adapter never invents a terminal fact.

The function boundary is the minimum automatic source resolution. When it is
too coarse, split the function at the boundary exposed by feedback instead of
tracing an inner expression or block. Function records share identity only
through caller-managed Context, independently of normalized product surfaces.

## Inspection

CLI inspection is a readonly projection over a JSONL Atom stream. It groups
records carrying `locus.trace` by that identity without inventing trace start,
end, lifecycle, or ownership. Records without the role remain valid Log
records and stay outside trace laws.

The cold-start size law counts the input bytes of every record in one trace and
finds a trace beyond 8 KiB. The prefix law excludes the Locus envelope, flattens
Source and opaque payload scalar values in stable order, and compares content
prefixes from eight through sixty-four bytes. At least five Atoms are required;
a prefix repeated by more than eighty percent of all Atoms is a finding.

JSON syntax, time, context, choices, and collection provenance do not contribute
to prefix density. Findings carry measurements rather than attribution or
repair advice. They cannot change input, acceptance, reporting, or diagnostics.
Malformed JSONL refuses complete inspection instead of producing a partial
clean result.

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

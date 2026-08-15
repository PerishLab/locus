# Hot paths

## Instrument a product

Read the product's own instructions and normal verification first, then follow
one bounded adapter path: add the Locus dependency to the crate holding the
traced declarations; create one isolated module carrying a readonly Engine and
Context seat, the Plumb-cascade gate, bounded collector chain, and file reporter;
call its `start` first in `main`; attach `#[locus::trace(with = view())]` to
three or four core-path function declarations.

`PRODUCT_LOCUS_REPORT` is the entire gate and an absent path is a muted run.
Read it through the Plumb cascade: a guarded product refuses raw environment
syntax outside its granted paths.

Prove transparency before landing by running the same command muted and observed
and requiring an identical exit status, stdout, and stderr. Then consume the
report, run the product's guard, and delete the adapter whole.

## Query and inspect history

Consume explicit Atom JSONL through stdin. Use `query ROLE` to enumerate sorted
identities with record counts, and add an exact key to replay matching logical
Atoms in input order. Read the final `locus.query/v1` summary for total,
matched, and identity coverage. Empty or unmatched input is a successful query
with explicit zero coverage; malformed input refuses without partial output.

Use `span` to pair entering and returning source records into elapsed and held
time per declaration. It names every span that entered and never returned, and
refuses where spans overlap without nesting. That refusal is a `tangled` record
naming the trace, the crossing window, and the declarations dropped inside it;
spans clear of that window still derive, so a concurrent burst costs its own
window rather than the file.

Use `inspect ROOT` only when the product root carries `locus.toml`. Inspection
composes Locus-owned mappings with product-declared thresholds and emits
measurements, never cause or repair advice. Its mapping kinds are
`representation-bytes`, `dominant-content-prefix-percent`, and
`held-time-nanoseconds`; the last runs the span derivation, so its summary
`refused` count matches what `span` drops on the same input. It groups by the
entering record's own Context, so bind a role onto source records before
expecting a group finer than the trace. Read neither command as process
liveness, executor ownership, parenthood, lifecycle, or business authority.

## Change Locus itself

Read `AGENTS.md` and `DESIGN.md`. Put Context, Atom, collector,
generator, reporter, hook, and diagnostic mechanics in `crates/locus`, source
adaptation in `crates/macro`, stdin-first consumption in `crates/cli`. Write the
law before or with its mechanism, and keep product analyzer identities,
thresholds, endpoints, credentials, and retention out of the substrate. Run
`plumb doctor .`, `ectropy .`, and the complete guard.

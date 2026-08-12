# Hot paths

## Instrument a product

Read the product's own instructions and normal verification first, then follow
`docs/run/adapter.md` in the Locus repository: one dependency, one isolated
module carrying the seat, gate, and policy, `start` as the first statement of
`main`, and `#[locus::trace]` on three or four core-path declarations.

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

Use `inspect ROOT` only when the product root carries `locus.toml`. Inspection
composes Locus-owned mappings with product-declared thresholds and emits
measurements, never cause or repair advice. Read neither command as process
liveness, executor ownership, parenthood, lifecycle, or business authority.

## Change Locus itself

Read `AGENTS.md`, then `docs/model/laws.md`, `docs/model/vocabulary.md`, and
`docs/run/verify.md` as the change requires. Put Context, Atom, collector,
generator, reporter, hook, and diagnostic mechanics in `crates/locus`, source
adaptation in `crates/macro`, stdin-first consumption in `crates/cli`. Write the
law before or with its mechanism, and keep product analyzer identities,
thresholds, endpoints, credentials, and retention out of the substrate. Run
`plumb doctor .`, `ectropy .`, and the complete guard.

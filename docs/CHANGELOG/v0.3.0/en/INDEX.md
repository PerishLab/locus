# Locus 0.3.0

## Refusing a crossing no longer refuses the file

`locus span` read containment inside one trace and refused the whole derivation
wherever two spans overlapped without nesting. On a real twenty-four hour
report that rule returned nothing at all: thirty-nine crossings out of a hundred
and eighty-four thousand closed spans, all inside one sub-second cluster,
discarded every unrelated reading in the file.

The cause is not a defect in any product's instrumentation. A trace is a
semantic role, not a process, so concurrent work under one trace interleaves by
construction, and an agent issuing parallel commands produces exactly this
shape. The premise the derivation needs is violated by ordinary operation, so it
recurs.

The refusal is now scoped to the time the crossing reaches. Every span meeting
that window is named in a `tangled` record carrying the trace, the window, and
the declarations dropped inside it; every span clear of the window still
derives. On that same report the scope costs three hundred and two spans out of
a hundred and ninety-six thousand.

The scope claims nothing wider than before, because containment closes it: any
span enclosing a dropped span meets the same crossing and is dropped with it, so
a surviving span never subtracts a child that was refused. Recovering the
crossing itself would take a parent link the records do not carry, and guessing
one would be the attribution this derivation exists to refuse.

## Inspection can measure where the time went

The mappings measured encoded bytes and dominant content prefix, so a product
could ask how large its records were but not where its time went.

A third mapping, `held-time-nanoseconds`, reports held and inclusive time per
Context group against a product-declared threshold. It runs the span derivation
rather than a second one, so a mapping and the `span` command refuse the same
crossings and drop the same spans; the inspection summary now counts what was
refused, so a threshold is never read against a silently shortened total.

A group key comes from the entering record's own Context and is never inherited
from an enclosing frame. A product that binds no role onto its source records
therefore groups only by trace, which is a statement about that product's
Context rather than a gap in the mapping.

The substrate is unchanged. No parent role, lifecycle, attribution, or product
vocabulary enters the Atom stream; every derived quantity stays downstream of
acceptance and stdin-first. The Plumb lock advances to 0.20.0.

# Locus 0.2.2

Locus CLI now derives span timing through `locus span`. It pairs one entering
source record with its returning record and reports elapsed and held time for
each traced declaration, ending with a `locus.span/v1` summary covering input,
matched records, declarations, and unclosed spans.

Held time reads containment inside one trace as the only nesting evidence the
records carry. Spans that overlap without nesting refuse the whole derivation
rather than dividing a duration the stream cannot apportion, so a concurrent
seat receives an explicit refusal instead of a quietly wrong number.

A span that entered and never returned is named rather than dropped. Panic,
abort, cancellation, and process loss all leave that shape, and the declaration
they died in is exactly what a reader wants.

The substrate is unchanged. No parent role, lifecycle, attribution, or product
vocabulary enters the Atom stream; every derived quantity stays downstream of
acceptance and stdin-first.

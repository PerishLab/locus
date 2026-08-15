# Restrained scenarios

## A traced declaration never records its return

A declaration that reaches `std::process::exit` records `enter` and no `return`,
exactly as a panic does. Trace below the exit boundary, or raise the exit into
`main`. `Err` is a normal return and records one.

## Spans arrive as siblings rather than a tree

Frames derive from the process seat, so nesting, duration, and attribution stay
downstream derivations. Do not add a parent role to recover a tree; that would
move a derived quantity into the substrate. `locus span` is that derivation:
it reads containment inside one trace and refuses when spans overlap without
nesting, rather than apportioning a duration the records cannot support.

## The product guard was already green

A guard that passed before the adapter is not evidence. Rerun it with the
adapter present, because the adapter adds syntax the product's own structural
and vocabulary laws will judge.

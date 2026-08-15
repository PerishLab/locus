# Migrating to Locus 0.2.2

Locus 0.2.2 preserves the 0.2.1 substrate. Context, Atom, collector, generator,
reporter, hook, and query behaviour are unchanged, and an existing report reads
without conversion.

Nothing is owed by a product that only accepts and reports records. The library
and the macro carry the release version without an interface change; a consumer
that pins `=0.2.1` may advance the pin at its own pace.

`locus span` is additive. It needs no product root and no analyzer declaration:

```sh
locus span < atoms.jsonl
```

A stream whose spans overlap without nesting refuses with exit code 2 and emits
no partial output. That refusal is the intended result, not a regression: held
time is underivable from records that carry no parent, and a divided guess
would be worse than none.

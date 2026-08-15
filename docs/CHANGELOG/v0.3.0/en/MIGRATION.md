# Migrating to Locus 0.3.0

Locus 0.3.0 preserves the 0.2.2 substrate. Context, Atom, collector, generator,
reporter, hook, and query behaviour are unchanged, and an existing report reads
without conversion.

Nothing is owed by a product that only accepts and reports records. The library
and the macro carry the release version without an interface change; a consumer
that pins `=0.2.2` may advance the pin at its own pace.

Three consumption surfaces changed. A reader of `locus span` or `locus inspect`
output owes an update.

## `locus span` no longer exits 2 on a crossing

In 0.2.2 a stream whose spans overlapped without nesting refused with exit code
2 and emitted no output. It now exits 0 and emits its derivation, with each
crossing window reported as a record:

```json
{"schema":"locus.span/v1","kind":"tangled","trace":"...","at":0,"until":30,
 "spans":2,"declarations":["m::inner","m::outer"]}
```

A caller that treated a nonzero exit as "this stream has a crossing" must read
the summary instead:

```sh
locus span < atoms.jsonl | tail -1
```

`refused` is zero exactly when no span was dropped. A caller that treated a
nonzero exit as "no output is coming" must stop discarding stdout.

Exit code 2 still means a refusal, and the refusals that keep it are unchanged:
a span that returns without entering, returns from another declaration, or
returns before it enters. Those reject the input itself rather than a region of
it.

## `locus.span/v1` summary carries two more fields

The summary gains `tangled`, the number of crossing windows, and `refused`, the
number of spans dropped inside them. A reader with an exact-shape assertion on
the summary object must widen it. Every previously emitted field keeps its name
and meaning.

## `locus.inspect/v1` summary carries `refused`

The inspection summary gains `refused`, the number of spans a held-time mapping
dropped. It is zero for a configuration that declares no held-time analyzer, so
an existing `locus.toml` sees only the new field.

## `held-time-nanoseconds` is a new mapping kind

Declaring it is optional; an existing configuration is unaffected. The threshold
is an inclusive nanosecond bound on held time per Context group:

```toml
[[analyzer]]
id = "trace.held"
mapping = { kind = "held-time-nanoseconds", group = "locus.trace" }
threshold = { above = 1000000000 }
```

The group key is read from the entering record's own Context. A product whose
source records carry no product role can only group by `locus.trace`; binding a
role onto those records is a product decision on its own acceptance surface and
is not owed by this release.

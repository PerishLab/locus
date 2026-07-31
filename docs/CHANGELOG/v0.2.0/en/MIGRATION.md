# Migration

`locus inspect` now requires a product root carrying `locus.toml`. Copy or
adapt the declaration in the Locus repository root, then pass that product root
as the optional positional argument:

```sh
locus inspect /path/to/product < atoms.jsonl
```

Consumers of stdout must accept `locus.inspect/v1` records. A finding now names
its analyzer, group, measurement, and threshold; every successful inspection
ends with a summary record. Clean inspection therefore no longer means empty
stdout.

The `locus` and `locus-macro` Rust APIs require no source migration. Their
versions move with the coupled release.

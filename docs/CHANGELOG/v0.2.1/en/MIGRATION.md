# Migration

Existing Rust API consumers require no source migration. The `locus`,
`locus-macro`, and `locus-cli` crate versions continue to move as one coupled
release.

CLI consumers may opt into the new query command:

```sh
locus query locus.trace < atoms.jsonl
locus query locus.trace trace-key < atoms.jsonl
```

New Unix report files are now owner-only. Operators that intentionally share a
report with a group must apply that policy after creation or provide a
pre-existing endpoint with the desired permissions. Locus does not modify the
mode of an existing report file.

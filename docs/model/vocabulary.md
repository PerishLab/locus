# Vocabulary

The repository starts with the smallest stable terms required by the first
vertical slice.

- `locus` — the fact authority and Rust anchor crate.
- `context` — one immutable effective role-to-key view.
- `role` — a semantic key position such as trace or span.
- `key` — an opaque identity value selected for one role.
- `candidate` — one proposed record before resolution and acceptance.
- `atom` — one accepted immutable record.
- `origin` — explicit, inherited, or generated selection provenance.
- `engine` — the owner of acceptance, reporting, and diagnostic execution.
- `policy` — typed declarative control values interpreted at bootstrap.
- `config` — policy plus the caller-visible observation hook.
- `collector` — a Locus-owned readonly observation algorithm explicitly
  selected and bound by the caller.
- `collection` — provenance for one collector value frozen into an Atom.
- `environment` — a collector selecting one exact environment variable.
- `argv` — a collector selecting one exact argument position.
- `process` — a collector selecting one exact process field.
- `generator` — a Locus-owned algorithm selected when a role is absent.
- `random` — the default generator algorithm.
- `shared` — the shared-file generator algorithm.
- `reporter` — a Locus-owned Atom delivery algorithm.
- `file` — the first append-only JSONL reporter.
- `hook` — a read-only observation sink.
- `adaptor` — the default terminal diagnostic handoff.
- `record` — the source macro and one logical append operation.
- `source` — optional code location captured by the macro.
- `function` — the minimum source boundary accepted by automatic tracing.
- `edge` — a function entry or normal return source fact.
- `outcome` — a post-reporting observation.
- `diagnostic` — an engine abnormality handed off outside reporting.
- `bootstrap` — freeze config and construct one engine.

# Codex observations

`locus adapt codex` accepts a Codex hook on stdin. It records the original
whitelisted hook payload and explicit session/turn/tool/agent context. Hook
records have `codex.record=hook.v1`; older unmarked hook records remain readable.
A Bash `PreToolUse` also produces at most one independent, source-less record
with `codex.record=command.v1` and the same session/turn/tool correlation keys.
It is a submitted command shape, not effective argv, startup or completion.

The command payload is an ordered tuple:

```json
[["GODEBUG=http2client=0"], "runseal-env", "gh", "api"]
```

It deliberately has no common event/schema/model envelope in front of the
measured content. The contract version is in the context selector. The fields
are explicit override markers, wrapper, program and operation, in that order.
Only public finite constants are retained. The exact explicit override shown
above is allowlisted; every other assignment becomes `other`, without its name
or value. An inherited environment is never read. Runseal seat names and
arbitrary operands, credentials, paths, prompts, responses and hashes are not
retained. No command text is executed by the adapter.

Supported programs are literal `gh`, `concord`, `plumb` and `santi`, optionally
wrapped by one literal `runseal :seat` and/or one simple `env` assignment prefix.
Absolute program paths, global option prefixes, nested wrappers and unsupported
operations are unknown. The bounded shell reader refuses control operators,
expansions and multiline commands; quoted literal operands remain operands.
The inspection budget is 4096 command bytes, 32 tokens and 128 bytes per token.
No more than 30 finite override markers fit. Accepted command Atoms are under
8192 serialized bytes for bounded hook identifiers; normalization never stores
arbitrary tokens. Program-specific operations are a small public vocabulary:

- gh: api, issue, pr, help, version.
- concord: issue, acceptance, member, help, version.
- plumb: release, land, metadata, help, version.
- santi: the existing simple-command operation profile (health, receipt,
  strand operations, turn show/stop, job get/query and help).

Known records have `codex.command=known.v1`, `codex.coverage=known` and wrapper
`none`, `env`, `runseal` or `runseal-env`. Unknown records have
`codex.command=unknown.v1`, wrapper `opaque` and a bounded coverage reason:
command-unavailable, budget, shell-structure, shell-expansion, shell-unparsed,
env-unparsed, program-unresolved or operation-unresolved. Their fixed opaque
tuple preserves coverage, not an assertion that the underlying commands match.

## Inspect one bounded slice

Create a consumer-owned directory containing a `plumb.toml` with this `[locus]`
section (a legacy standalone `locus.toml` with the same fields remains supported
when `plumb.toml` has no `[locus]` section; keeping both refuses):

```toml
[locus]
version = 1

[[locus.analyzer]]
id = "codex.command.prefix"
mapping = { kind = "dominant-content-prefix-percent", group = "codex.turn" }
threshold = { above = 80 }

[[locus.analyzer]]
id = "codex.command.size"
mapping = { kind = "representation-bytes", group = "codex.turn" }
threshold = { above = 8192 }
```

Use an explicit producer, receive-time window and known-command selector:

```sh
locus analyze /path/to/consumer --producer locus-adapter-codex --from START --to END --select codex.command=known.v1
locus query codex.turn TURN --producer locus-adapter-codex --from START --to END --select codex.command=known.v1
```

Replace START/END with zoned RFC3339 instants or nanoseconds. Grouping by
`codex.session` instead of `codex.turn` is also supported. These group identities
come from the hook; they do not create function spans or execution durations.
The CLI supports one exact read selector; after selecting known commands,
query's role/key argument can narrow the group. For a wrapper-specific read,
select `codex.wrapper=env` or `runseal-env`; only known records carry those keys.
Use `query codex.coverage` with `--select codex.record=command.v1` to inspect
unknown coverage as well. Never mix unknown tuples into a known-command claim.

The existing prefix mapping needs at least five Atoms, compares content prefixes
of 8–64 bytes, and uses a strictly-greater-than threshold: 4/5 is not above80,
5/6 is. The denominator is selected accepted records, not distinct invocations.
Repeated deliveries remain observations; changing order does not change dominant
prefix occupancy. Ordinary repeated operations can trigger a finding. Neither
a shared wrapper nor an unknown override proves the exact commands match,
that a workaround is obsolete, or that execution is wasting time.

`analyze` emits bounded evidence references and an input/configuration summary.
Evidence samples do not include payload and are not prefix attribution. Use
`query` above to read the matching facts before deciding what to investigate.
`tree` only expands real trace/span/parent evidence; it cannot expand these
untraced hook records into an invented process tree. Missing final summaries,
retention and drain delay remain explicit limitations, not clean results.

## Reconcile capture separately

```sh
locus quality codex --from START --to END --diagnostics /private/adapter.log --expect-session SESSION
```

An invocation writes one complete stderr line: hook-only v1, or paired v2 with
separate `hook` and `command` v1 outcomes. Each has its own attempt identity,
receive time, optional accepted Atom ID, buffer status and bounded diagnostics.
The adapter keeps stdout `{}` and exit0, including collection failures. A
rejection or buffer failure for one record does not undo acceptance of another.
Older diagnostic readers refuse v2; use the containing CLI for mixed windows.

Quality keeps hook records, before/after pairing and original outcomes at the
root. `commands` has separate record counts, coverage in `classes`, outcomes
and exact-ID readback. It has no synthetic before/after pair. A Bash PreToolUse
normally produces two Atoms but one call; non-Bash and other hook events produce
one. Either projection can be missing or fail independently. A missing outcome
line/process loss stays unknown, and no report promises a source-event ID or
exactly-once delivery. A successful buffer handoff is not central persistence;
wait for ordinary reporting and verify exact IDs, without manually flushing.

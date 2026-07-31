# Locus 0.2.0

Locus inspection now treats analyzers as product declarations rather than
compiled policy. A root `locus.toml` composes Locus-owned mappings with
thresholds and binds each analyzer to an explicit Context role.

The first mapping catalog contains encoded representation bytes and dominant
decoded content-prefix percentage. The Locus root declaration preserves the
existing 8 KiB size and eighty-percent repetition coarse sieves.

Every complete inspection now emits `locus.inspect/v1` JSONL findings followed
by one summary naming analyzer coverage, input record count, and finding count.
Empty declarations report zero coverage instead of implying clean analysis.

Configuration is compiled before stdin is consumed. Unknown fields, mapping
kinds, invalid roles, duplicate analyzer identities, malformed Atoms, and I/O
failure refuse without a partial report.

# Locus 0.2.1

Locus CLI now provides exact readonly Atom retrieval through
`locus query <ROLE> [KEY]`. A role-only query lists sorted identities and
record counts. Adding a key replays matching logical Atoms in input order.

Every complete query ends with a `locus.query/v1` summary covering input,
matches, and identities. Input is fully validated before output, so malformed
JSONL refuses without a partial result. Query remains stdin-first and requires
neither a product root nor analyzer declarations.

On Unix, the file reporter now creates a new report with owner-only `0600`
permissions. Existing report permissions remain unchanged. The temporary
Locus skill records the one-way, default-muted adapter convention for products
that adopt collection before a source-transparent delivery mechanism exists.

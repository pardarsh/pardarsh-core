# ADR 0007: Storage behind a narrow trait

Status: accepted (2026-10-09)

## Decision
The `Store` trait has seven operations: origin, get snapshot, get history,
look up by idempotency key, atomic commit (event + snapshot, with sequence
and key checks and redaction scrubbing), list with a simple filter, and
incoming relations. Validation, projection and authorization stay out of
stores. The SQLite store keeps JSON documents plus indexed columns
(`pd_records`, `pd_events`, `pd_relations`); its schema is private.

## Consequences
Text search and complex filtering are application concerns for now (the
first app filters in memory). A Postgres store, or search indexing, can be
added without changing callers. Every store must pass the shared repository
test suite (`tests/repository.rs` runs against both stores).

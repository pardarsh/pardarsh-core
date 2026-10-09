# History, concurrency and schema versions

## History

Each record has an append-only event log with gap-free sequences starting at
1. The stored snapshot is always the projection of the log, and bundles are
verified by replaying it. Corrections, retractions, supersession and relation
retractions are events; nothing is deleted. Redaction is the one exception
(ADR 0006).

## Concurrency

Optimistic. A command may carry `expected_version`; the repository rejects it
with `Conflict` if the record moved on. Stores additionally reject any event
whose sequence is not exactly `version + 1`, so concurrent writers cannot fork
a history even without `expected_version`.

## Idempotency

A command may carry an idempotency key (up to 128 characters of
`[A-Za-z0-9-_.:]`). Repeating a command with the same key, actor and payload
returns the original result (`replayed: true`) without writing. Reusing the
key for a different command fails with `IdempotencyConflict`. Payload
comparison ignores server-generated ids and times. For `create`, the key and
actor alone identify the command. Applications should scope keys per account.

## Schema compatibility

Versions are `MAJOR.MINOR`. A reader at `R` reads data written at `W` if and
only if `W.major == R.major && W.minor <= R.minor`. Minor versions may only
add optional fields, kinds, relations or enum values; anything else is a major
version. Data from a newer minor is rejected rather than silently truncated.
Each extension namespace is versioned independently with the same rule. The
envelope is at 1.0 and the bundle format at 1.0.

## Changing the shared schema

Propose the change in an ADR, add conformance fixtures for the new accept and
reject cases, bump the minor version for additive changes, and record it in
`CHANGELOG.md`.

# ADR 0003: Origin-qualified UUIDv7 identifiers

Status: accepted (2026-10-09)

## Decision
Record ids are `pd:<origin>:<uuidv7>`. The origin is a lowercase DNS-like
name of the minting instance. UUIDv7 is time-ordered (useful for listing) and
needs no coordination. Only the canonical lowercase hyphenated form parses,
so one record has exactly one spelling. Event, relation and source ids are
plain UUIDv7.

## Consequences
Remote records can be referenced before federation exists. Ids leak
approximate creation time (acceptable: records carry `recorded_at` anyway).
Changing an instance's origin requires migration; the SQLite store refuses to
open a database under a different origin.

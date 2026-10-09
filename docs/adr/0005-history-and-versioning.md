# ADR 0005: Append-only events with projected state

Status: accepted (2026-10-09)

## Context
The brief asks to preserve history, treat status as a projection, and define
idempotency and concurrency.

## Decision
* Every change is an event with a gap-free per-record sequence; the current
  record is the projection of its events. Stores persist both the events and
  the latest snapshot (snapshot for fast reads, events as truth).
* Event kinds are generic (created, amended, corrected, relation
  added/retracted, retracted, superseded, redacted, annotated) and may carry an
  application tag such as `app:status_change` and a reason. Corrections,
  retractions, supersession and redactions require a reason.
* Optimistic concurrency via `expected_version`, and stores reject any
  non-contiguous sequence. Idempotency keys are unique per store and replay
  the original result.
* Every command's resulting state is fully validated before commit.
* Schema compatibility: `MAJOR.MINOR`, readers accept same major and older or
  equal minor (see `docs/versioning.md`).

## Consequences
This is event sourcing per record, not a global event log; there is no
cross-record transaction. A command that touches two records (for example a
decision plus the `decided_by` link on the proposal) is two commits.

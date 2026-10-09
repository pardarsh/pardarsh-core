# ADR 0004: Common envelope with typed, namespaced extensions

Status: accepted (2026-10-09)

## Decision
* One envelope for all records (see `docs/data-model.md`), with a small core
  kind vocabulary that is deliberately not issue-centric, plus namespaced
  extension kinds.
* Application fields are extensions: a namespace with a versioned schema and
  typed fields. Unknown namespaces or fields are rejected; there is no untyped
  JSON blob.
* Relations come from a controlled vocabulary with documented direction,
  meaning and validation, plus namespaced extension relations without core
  rules. Every relation carries a certainty so links never imply more than
  their sources.
* Times are separate fields: recorded, published, effective from/until,
  observed. Event times are separate again: occurred versus recorded.

## Consequences
Applications cannot invent a competing model; they extend this one. Core
relation rules that need the target kind are enforced only for local targets.

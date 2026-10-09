# Data model (envelope version 1.0)

## Record envelope

| Field | Type | Notes |
| --- | --- | --- |
| `id` | `pd:<origin>:<uuidv7>` | Assigned by the repository. Canonical lowercase form only. |
| `kind` | string | Core kind or `namespace:name` extension kind |
| `schema_version` | `MAJOR.MINOR` | Envelope version the record was written with |
| `title` | string | Required, at most 300 chars, no control characters |
| `summary`, `body` | string, optional | At most 2 000 / 50 000 chars |
| `times.recorded_at` | timestamp | When this system recorded the record (ingestion time) |
| `times.published_at`, `effective_from`, `effective_until`, `observed_at` | timestamp, optional | Claims about the world; never conflated with `recorded_at` |
| `actors[]` | `{role, actor, disclosure}` | Roles: `creator`, `author`, `reporter`, `issuer`, `publisher`, `reviewer`, `subject`, `responsible`, `contributor`, or an extension role |
| `sources[]` | `SourceRef` | See `provenance.md` |
| `relations[]` | `Relation` | Outgoing typed edges |
| `visibility` | `public` or `restricted{audiences}` | Empty audiences means privileged viewers only |
| `lifecycle` | `active`, `retracted`, `superseded{by}` | Generic lifecycle only; app workflow lives in extensions |
| `extensions` | `{namespace: {schema_version, fields}}` | Typed values validated against registered schemas |
| `redactions[]` | `FieldPath` | Which values were redacted |
| `version` | integer | Sequence of the last applied event |
| `updated_at` | timestamp | Recorded time of the last event |

Actor references are `{"type":"record","id":...}`, `{"type":"anonymous"}`
or `{"type":"system","name":...}`. `disclosure: withheld` hides the identity
from non-privileged viewers while retaining it for accountability.

## Extension values

Typed and externally tagged: `{"text": "..."}`, `{"integer": 3}`,
`{"boolean": true}`, `{"timestamp": "..."}`, `{"date": "2026-07-10"}`,
`{"enum": "resolved"}`, `{"reference": "pd:..."}`, `{"list": [...]}`, or
`"redacted"`. Field types: `text{max_len}`, `integer{min,max}`, `boolean`,
`timestamp`, `date`, `enum{values}`, `reference{kinds}`, `list{item,max_items}`.
Unknown namespaces and unknown fields are rejected.

## Relations

Stored on the source record. Each has `id`, `kind`, `target`, optional
`target_locator` (for remote targets), `certainty`, `asserted_by`,
`asserted_at`, optional `sources` and `note`, and `retracted`.

| Kind | Meaning (source to target) | Rule |
| --- | --- | --- |
| `supports`, `contradicts` | offered as support / contradiction | |
| `responds_to` | is a response to | |
| `duplicates` | describes the same thing; target is canonical | same kind |
| `supersedes` | replaces | same kind |
| `corrects`, `retracts` | corrects an error in / withdraws | |
| `derived_from`, `cites` | was derived from / cites | |
| `proposes_change_to` | proposes a change to | |
| `decided_by` | was decided by | target is a `decision` |
| `issued_by`, `assigned_to`, `owned_by` | as named | target is an `actor` |
| `implements`, `funded_by`, `procured_through`, `affects`, `located_at` | as named | |
| `fulfills` | fulfills obligation | target is an `action` |
| `measured_by` | is measured by | target is `outcome` or `evidence` |
| `resulted_in`, `disputes`, `related_to` | as named | |
| `namespace:name` | extension relation | no core rules |

All relations: no self-links; local targets must exist; no identical active
duplicate; `sourced` certainty requires at least one source. Remote targets
are accepted and reported as unresolved by traversal.

## Events

`{id, record_id, sequence, kind, attribution, occurred_at, recorded_at,
idempotency_key?, reason?, tag?, visibility}` where `kind` is one of
`created{record}`, `amended{changes}`, `corrected{changes, corrects?}`,
`relation_added{relation}`, `relation_retracted{relation_id}`, `retracted`,
`superseded{by}`, `redacted{fields}`, `annotated{name, data}`.

Changes: `title`, `summary`, `body`, `published_at`, `effective_from`,
`effective_until`, `observed_at`, `visibility`, `add_actor`, `add_source`,
`set_extension_field{namespace, field, value?}`.

`corrected`, `retracted`, `superseded` and `redacted` require a reason.
`occurred_at` may not be later than now (or `recorded_at`) plus 5 minutes of
clock skew. Retracted records accept only corrections, redactions and
annotations.

## Serialization

JSON via serde. Hashes use canonical JSON (sorted keys, no whitespace); see
`src/interchange.rs`. The fixtures in `fixtures/` are complete examples.

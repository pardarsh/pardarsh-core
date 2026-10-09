# pardarsh-core entities

This guide explains every model in pardarsh-core: what it is for, and an
example of what it looks like. JSON examples show the real serialized form
(ids are shortened as `pd:city.example.org:…`).

pardarsh-core defines the shapes; an application only fills them in, with its
own typed fields added as namespaced extensions. The examples use a
hypothetical civic-issue app whose extension namespace is `app.issue`.

## 1. Identity

#### `Origin` (`src/id.rs`)
**Purpose:** the name of the instance that created a record, like a domain.
It goes into every id, so records from different instances never collide and
can point at each other later (federation).
**Example:** `city.example.org`, `budget.state.gov.example`.
Rules: lowercase letters, digits, `-` and `.` only.

#### `RecordId`
**Purpose:** the permanent, globally unique id of a record. It never changes,
even if the record is edited, corrected or retracted.
**Format:** `pd:<origin>:<uuidv7>`. UUIDv7 is time-ordered, so newer ids sort later.
**Example:** `pd:city.example.org:01a1213a-24dd-76e0-aaf1-68511ce1e443`

#### `EventId`, `RelationId`, `SourceId`
**Purpose:** ids for the smaller things inside a record: one history event,
one link, one source reference. Each is a plain UUIDv7.
**Example:** a moderator redacts *one* photo link by naming its `SourceId`.

#### `NamespacedName`
**Purpose:** a `namespace:name` label that lets applications add their own
vocabulary without polluting the core.
**Example:** `app:status_change` (an event tag),
`app:moderation_report` (a record kind), `procurement:awarded_to` (a relation).

## 2. The record

#### `Record` (`src/model.rs`), the common envelope
**Purpose:** the single shape every public record shares, whether it is an
issue, a court decision, a budget line or a person. It is the *current
state*, rebuilt from the record's history (see Events).

| Field | Meaning |
| --- | --- |
| `id`, `kind`, `schema_version` | who it is, what type it is, which format version |
| `title`, `summary`, `body` | human text |
| `times` | `RecordTimes` (below) |
| `actors` | who is involved and in what role |
| `sources` | provenance: where the information comes from |
| `relations` | typed links to other records |
| `visibility` | who may see it |
| `lifecycle` | active, retracted or superseded |
| `extensions` | app-specific typed fields |
| `redactions` | which fields were redacted |
| `version`, `updated_at` | number of the last change, and when it was recorded |

**Example (an issue):**
```json
{
  "id": "pd:city.example.org:01a1…",
  "kind": "issue",
  "schema_version": "1.0",
  "title": "Streetlight out on 4th Cross for two weeks",
  "body": "The light near the bus stop has been off since mid-June.",
  "times": { "recorded_at": "2026-07-01T18:00:00Z" },
  "actors": [
    { "role": "reporter", "actor": { "type": "record", "id": "pd:city.example.org:01a0…asha" }, "disclosure": "withheld" }
  ],
  "sources": [],
  "relations": [],
  "visibility": { "level": "public" },
  "lifecycle": { "state": "active" },
  "extensions": {
    "app.issue": {
      "schema_version": "1.0",
      "fields": {
        "status": { "enum": "submitted" },
        "category": { "enum": "streetlights" },
        "locality": { "text": "Ward 7" }
      }
    }
  },
  "version": 1,
  "updated_at": "2026-07-01T18:00:00Z"
}
```

#### `RecordKind`
**Purpose:** the type of a record. The core kinds are deliberately broader
than issues, so the same infrastructure can hold laws, decisions or contracts.

| Kind | Purpose | Example |
| --- | --- | --- |
| `issue` | a problem, question, service failure or public concern | "No water supply in Block C for three days" |
| `evidence` | material relevant to something (photo, URL, dataset, observation) | "Photo of the dark street at 8pm" |
| `claim` | an attributed statement that may be supported or disputed | Ward office: "LED fitting installed on 8 July" |
| `proposal` | a suggested remedy or change | "Replace the fitting with an LED lamp"; a bill |
| `comment` | question, argument, objection, response or review | "Is there a maintenance contract for this road?" |
| `decision` | a formal or attributable choice | "Accepted: LED fittings are in this year's plan" |
| `action` | a committed or required next step (obligation) | "Replace fitting at 4th Cross, due 10 July" |
| `project` | implementation activity / work item | "Ring Road resurfacing, phase 2" |
| `outcome` | an observed or reported result in the world | "Light flickers and goes off after 9pm" |
| `actor` | person, institution, department, group, contractor | "Ward 7 Public Works Office" |
| `source` | a published source artifact | "Municipal Act, s. 42" |
| `ns:name` | an app-defined kind | `app:moderation_report` |

There is no `event` kind. Events are the history log of each record (section 1.5).

#### `RecordTimes`
**Purpose:** keep different meanings of "when" apart, so a record never
confuses when something happened with when it was typed in.

| Field | Meaning | Example |
| --- | --- | --- |
| `recorded_at` | when this system recorded it (always set, by the server) | 1 July, 18:00 |
| `published_at` | when the source published it | gazette date of a notice |
| `effective_from` / `effective_until` | when it applies | a rule in force from 1 April |
| `observed_at` | when someone observed it | photo taken at 20:10 |

#### `ActorRef`, `Attribution`, `Disclosure`
**Purpose:** say *who* did or said something, without forcing identity to be public.
* `ActorRef` is `record` (points to an `actor` record), `anonymous`, or `system` (an automated process).
* `Attribution` = `ActorRef` + `Disclosure`.
* `Disclosure::withheld` keeps the identity for accountability but shows it as
  "identity withheld" to the public. The person themself and moderators still see it.

**Example:** Asha reports an issue near her home but does not want her name
public: `{ "actor": {"type":"record","id":"pd:…asha"}, "disclosure": "withheld" }`.

#### `ActorRole` and `ActorRoleKind`
**Purpose:** attach actors to a record with an explicit role, because "who is
involved" means different things.
Roles: `creator`, `author`, `reporter`, `issuer`, `publisher`, `reviewer`,
`subject`, `responsible`, `contributor`, or an extension role.
**Example:** a decision has `issuer` = Ward 7 Public Works Office and
`author` = the Ward 7 engineer who typed it.

#### `Visibility`
**Purpose:** decide who may see a record, an event or a single source.
* `public`: anyone.
* `restricted { audiences }`: only viewers holding one of those audiences, plus privileged viewers. Empty audiences means privileged viewers only.

**Example:** a moderation report is `{"level":"restricted","audiences":["moderators"]}`.
A photo showing someone's house can be restricted while the issue stays public.

#### `Lifecycle`
**Purpose:** generic "is this record still current?", with history kept.
`active`, `retracted` (withdrawn by its author), `superseded { by }` (replaced by another record).
**Example:** "Municipal Act s. 42" becomes `superseded` by "s. 42 as amended by Act 17".
App workflow states such as `in_progress` do **not** live here; they are extensions.

#### `RecordDraft`
**Purpose:** the input for creating a record. The client supplies content; the
server assigns the id, version and recorded time, so they cannot be faked.
**Example (Rust):**
```rust
RecordDraft::new(RecordKind::Evidence, "Photo after dark")
    .source(SourceRef::new("https://photos.example.org/dark-street.jpg"))
    .relation(RelationDraft::new(RelationKind::Supports, issue_id))
```

## 3. Extensions (`src/extension.rs`)

#### `ExtensionSchema`, `FieldSpec`, `FieldType`, `ExtensionRegistry`
**Purpose:** let an application add its own fields to records *with types and
validation*, instead of inventing its own data model or storing an untyped blob.
* `ExtensionSchema`: a namespace, a version, which kinds it applies to, and its fields.
* `FieldSpec`: one field's type, whether it is required, and a description.
* `FieldType`: `text{max_len}`, `integer{min,max}`, `boolean`, `timestamp`, `date`, `enum{values}`, `reference{kinds}`, `list{item,max_items}`.
* `ExtensionRegistry`: the set of schemas the validator knows. Unknown namespaces or fields are rejected.

**Example:** An app registers `app.issue` for kind `issue` with
`status` (required enum), `category` (required enum), `location_text`
(text, up to 200 chars), `locality`, `affected_service`.

#### `ExtensionData` and `ExtValue`
**Purpose:** the actual values stored on a record, each tagged with its type.
**Example:** `{"enum":"in_progress"}`, `{"text":"Ward 7"}`, `{"date":"2026-07-10"}`,
`{"reference":"pd:…"}`, and `"redacted"` after a moderator removes a value.

## 4. Provenance (`src/provenance.rs`)

#### `SourceRef`
**Purpose:** record where information came from and how trustworthy that
origin is, without pretending to know more than we do. Unknown metadata is allowed.

| Field | Meaning |
| --- | --- |
| `locator` | URI: `https`, `http`, `urn` or `doi` only (`javascript:` etc. rejected) |
| `title`, `issuer`, `media_type` | what it is and who issued it |
| `published_at` vs `retrieved_at` | when it was published vs when we looked at it |
| `content_hash` | `sha256:…` of the exact bytes, if fetched |
| `authority` | `SourceAuthority` (below) |
| `derivation` | `Derivation` (below) |
| `submitted_by` | who attached it |
| `visibility` | can be stricter than the record (private attachment) |

**Example:**
```json
{ "id": "01a1…", "locator": "https://legis.example.org/acts/municipal/s42",
  "issuer": "State Legislative Assembly", "authority": "official",
  "derivation": { "type": "original" }, "retrieved_at": "2026-03-02T09:00:00Z",
  "visibility": { "level": "public" } }
```

#### `SourceAuthority`
**Purpose:** who stands behind the source. `official`, `third_party`,
`user_submitted`, `unknown`. "Official" means the institution published it,
not that every statement in it is true.
**Example:** a resident's photo link is `user_submitted`; a gazette notice is `official`.

#### `Derivation`
**Purpose:** distinguish the original from things made from it.
`original`, `extracted{method}`, `summary{author}`,
`machine_generated{method, reviewed}`.
**Example:** an AI summary of a tender would be
`{"type":"machine_generated","method":"model X summary","reviewed":false}`.

#### `ContentHash`
**Purpose:** prove content has not changed. Always `sha256:<64 hex>`.
For files it hashes the bytes; for records and bundles it hashes canonical JSON.

## 5. Relations (`src/relation.rs`)

#### `Relation`, `RelationKind`, `Certainty`, `RelationDraft`
**Purpose:** typed, directed links between records (`source --kind--> target`).
This is what makes it a graph. The link is stored on the source record.
* `RelationKind`: a controlled vocabulary, each with a meaning and rules (below).
* `Certainty`: how sure the link is: `unverified`, `asserted`, `sourced`
  (must cite at least one source), `disputed`. A link never claims more than its evidence.
* `retracted`: a link that was withdrawn stays in history but drops out of the graph.
* `RelationDraft`: the input form.

**Example:** the reporter guesses the responsible body:
```json
{ "kind": "assigned_to", "target": "pd:…ward7_works", "certainty": "unverified",
  "note": "suggested by the reporter; not confirmed",
  "asserted_by": { "actor": {"type":"record","id":"pd:…asha"}, "disclosure": "withheld" },
  "asserted_at": "2026-07-01T18:00:00Z", "retracted": false }
```

| Relation | Meaning | Rule | Example |
| --- | --- | --- | --- |
| `supports` / `contradicts` | evidence for / against | | photo `supports` streetlight issue |
| `responds_to` | a response | | comment `responds_to` issue |
| `duplicates` | same thing; target is canonical | same kind | issue B `duplicates` issue A |
| `supersedes` | replaces | same kind | new rule `supersedes` old rule |
| `corrects`, `retracts` | fixes / withdraws | | erratum `corrects` notice |
| `derived_from`, `cites` | based on / refers to | | amended act `derived_from` old act |
| `proposes_change_to` | proposed change | | bill `proposes_change_to` s. 42 |
| `decided_by` | decided by a decision | target must be `decision` | proposal `decided_by` decision |
| `issued_by`, `assigned_to`, `owned_by` | issuer / responsible / owner | target must be `actor` | issue `assigned_to` Ward 7 office |
| `implements`, `funded_by`, `procured_through` | delivery and money links | | project `funded_by` budget line |
| `affects`, `located_at` | impact / place | | outage `affects` Block C |
| `fulfills` | meets an obligation | target must be `action` | work order `fulfills` action |
| `measured_by` | measured by | target `outcome` or `evidence` | project `measured_by` inspection |
| `resulted_in`, `disputes`, `related_to` | as named | | bill `resulted_in` amended act |
| `ns:name` | app-defined | none | `app:flags` |

Every relation: no self-links, local targets must exist, and no identical active duplicate.

## 6. History (`src/event.rs`, `src/projection.rs`)

#### `Event`
**Purpose:** one attributable change to one record. History is a list of
events numbered 1, 2, 3… with no gaps. Nothing is overwritten.
Fields: `id`, `record_id`, `sequence`, `kind`, `attribution` (who),
`occurred_at` (when it happened, per the actor) vs `recorded_at` (when the
server stored it), optional `idempotency_key`, `reason`, `tag`, and `visibility`.

**Example (a status change):**
```json
{ "sequence": 5, "kind": { "type": "amended", "changes": [
    { "field": "set_extension_field",
      "value": { "namespace": "app.issue", "field": "status", "value": { "enum": "resolved" } } } ] },
  "attribution": { "actor": {"type":"record","id":"pd:…ward7_engineer"}, "disclosure": "public" },
  "reason": "fitting replaced", "tag": "app:status_change",
  "occurred_at": "2026-07-08T11:00:00Z", "recorded_at": "2026-07-08T11:00:02Z",
  "visibility": { "level": "public" } }
```

#### `EventKind`
| Kind | Purpose | Example |
| --- | --- | --- |
| `created` | initial state (a full snapshot) | issue submitted |
| `amended` | ordinary edits | summary improved |
| `corrected` | fixes an error; reason required; may name the wrong event | "wrong day: cut since Monday" |
| `relation_added` / `relation_retracted` | link added / withdrawn | institution assigned / unassigned |
| `retracted` | record withdrawn; reason required | "filed in error" |
| `superseded` | replaced by another record; reason required | "merged into ward-level issue" |
| `redacted` | values removed from record *and* history; reason required | phone number removed |
| `annotated` | typed note that does not change the record | private moderation note |

#### `Change`
**Purpose:** one field-level edit inside `amended` or `corrected`: `title`,
`summary`, `body`, the four times, `visibility`, `add_actor`, `add_source`,
`set_extension_field`.
**Example:** `{"field":"title","value":"Pothole near Ring Road junction"}`.

#### `FieldPath`
**Purpose:** point at one redactable value: `title`, `summary`, `body`,
`source{id}`, `extension{namespace, field}`, `annotation{event, key}`.
**Example:** `{"path":"extension","namespace":"app.issue","field":"location_text"}`.

#### Projection, `FieldDiff`, `TimelineEntry`
**Purpose:** turn history into something usable.
* `project(events)` rebuilds the current `Record` by replaying events. This is
  why a status is "a projection, not the whole truth".
* `FieldDiff` is one readable change: `{"field":"app.issue.status","before":"in_progress","after":"resolved"}`.
* `TimelineEntry` is one event explained for humans: what happened, who, when, why, and the diffs.

## 7. Writing and validating (`src/repository.rs`, `src/validation.rs`)

#### `Repository`
**Purpose:** the only way to change records. Every command (`create`, `amend`,
`correct`, `link`, `unlink`, `retract`, `supersede`, `redact`, `annotate`)
becomes an event; the repository checks it, validates the resulting record,
checks link targets, and stores it. It does not decide *who* may do what;
that is the application's job.

#### `Meta`
**Purpose:** the "who, when, why" that goes with every command: attribution,
optional `occurred_at`, `idempotency_key`, `expected_version`, `reason`, `tag`, `visibility`.
**Example:** `Meta::by(engineer).reason("fitting replaced").tag(status_change).expect_version(7)`.

#### `Committed`
**Purpose:** the result of a command: the new record, the event, and
`replayed: true` if the same command was retried with the same idempotency
key (so a double-click creates one issue, not two).

#### `Validator`, `Limits`, `ValidationErrors`, `ValidationIssue`
**Purpose:** reject bad or hostile input with precise messages. Checks text
lengths and control characters, URL schemes, time order, relation rules,
extension types, and future timestamps.
**Example error:** `{"path":"sources[0].locator","message":"scheme must be one of https, http, urn, doi"}`.

#### `Clock` (`SystemClock`, `ManualClock`)
**Purpose:** where "now" comes from. The server's clock is used for
`recorded_at`, so clients cannot backdate records. `ManualClock` makes tests repeatable.

#### `SchemaVersion` (`src/schema.rs`)
**Purpose:** format versioning as `MAJOR.MINOR`. A reader accepts the same
major and an older or equal minor. Envelope and bundle formats are both 1.0.

#### `Error`
**Purpose:** typed failures: `Validation`, `NotFound`, `Conflict` (someone
changed the record first), `IdempotencyConflict`, `NotPermitted`, `Import`, `Storage`, and others.

## 8. Storage and graph (`src/storage`, `src/graph.rs`)

#### `Store`, `MemoryStore`, `SqliteStore`
**Purpose:** persistence behind a small interface, so the database can change
later. Stores save events and the latest snapshot atomically, refuse gaps in
sequences (concurrency safety), enforce unique idempotency keys, and scrub
history on redaction. `MemoryStore` is for tests and tools; `SqliteStore` is
the relational reference store.

#### `RecordQuery`, `IncomingEdge`
**Purpose:** simple listing (by kind, extension, active or not, paging) and
"what points at this record?".
**Example:** list every active `issue` carrying `app.issue`; find all
evidence that `supports` an issue.

#### `Edge`, `TraversalOptions`, `Traversal`, `Direction`
**Purpose:** follow links across records, with limits on depth and size.
`Traversal.unresolved` lists targets that live elsewhere (remote ids), so they
are shown as unknown rather than guessed.
**Example:** from Bill 17, two hops reach the provision it amends, the
committee decision, the objection, the response and the amended act.

## 9. Privacy and exchange (`src/access.rs`, `src/interchange.rs`, `src/federation.rs`)

#### `Viewer`
**Purpose:** describes who is reading (actor, audiences, privileged or not).
`view_record` and `view_history` remove what that viewer may not see:
restricted records, events and sources, and withheld identities.
**Example:** the public sees the issue without the private photo and with
"identity withheld"; a moderator sees both.

#### `Bundle`, `BundleEntry`, `Completeness`
**Purpose:** a portable export of records with their full history and
hashes, for backups, research, journalists and other instances.
`complete` bundles (privileged export) can be imported; `filtered` bundles
(public export) are for reading only.
**Example:** `GET /api/issues/{id}/export` returns a `pardarsh.bundle` with the
issue, its evidence, comments, decisions and their histories.

#### `ImportPolicy`, `ImportReport`
**Purpose:** treat incoming data as untrusted: size and count limits, hash
checks, history replay, origin allow-list, refusal to accept records that
claim to be local, divergence detection. Re-importing is idempotent.

#### `RemoteSource` (federation)
**Purpose:** the contract a future federation client will implement to fetch
bundles from another instance. There is no network implementation yet.

## Worked example: one streetlight, end to end

1. A resident reports "Streetlight out on 4th Cross" with her identity withheld
   and suggests the ward office: an `issue` record (with `app.issue.status =
   submitted`) plus an `assigned_to` relation marked `unverified`.
2. A neighbour attaches a photo: an `evidence` record that `supports` the issue,
   with a `user_submitted` `SourceRef`. He proposes an LED lamp: a `proposal`
   that `responds_to` the issue.
3. The ward office confirms responsibility (an `asserted` `assigned_to`) and
   changes the status: `amended` events tagged `app:status_change`, each with
   who, when and why.
4. The office decides: a `decision` `issued_by` the office, and the proposal
   gets `decided_by` that decision. It plans an `action`, then posts a `claim`
   that the work is complete.
5. A resident later records an `outcome` ("goes off after 9pm"). The claim and
   the outcome stay separate records, so readers can compare them.
6. A moderator redacts the exact location: a `redacted` event scrubs it from
   the record and all earlier events.
7. `project(history)` rebuilds the current state; `timeline` explains each
   step; `export` produces a verified `Bundle`, with the withheld identity and
   any restricted source removed for public viewers.

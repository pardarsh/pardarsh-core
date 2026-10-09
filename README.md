# pardarsh-core

Application-agnostic infrastructure for the **Public Decision Graph**: a shared
way to represent public issues, evidence, claims, proposals, decisions,
actions, outcomes, actors and sources as connected, attributable,
machine-readable records.

`pardarsh-core` is a Rust library. It owns the record model and the rules
around it; applications (such as a civic issue tracker) own their workflows,
UI, accounts and policy and depend on this crate, never the other way round.

| Module | Responsibility |
| --- | --- |
| `id` | Origin-qualified record ids (`pd:<origin>:<uuidv7>`), event/relation/source ids, namespaced names |
| `model` | Common record envelope, record kinds, actors and roles, visibility, lifecycle, times |
| `extension` | Namespaced, typed extension fields and their schemas |
| `provenance` | Source references, authority, derivation, content hashes |
| `relation` | Controlled relation vocabulary with direction, meaning, rules and certainty |
| `event` | Append-only, attributable events; field paths; redaction scrubbing |
| `projection` | Current state from history; human-readable diffs and timelines |
| `validation` | Structural and semantic validation, size limits, untrusted-input rules |
| `repository` | The command side: idempotency, optimistic concurrency, validation, reference checks |
| `storage` | `Store` trait, in-memory and SQLite reference stores |
| `graph` | Incoming/outgoing relationship queries, bounded traversal |
| `access` | Visibility and identity-disclosure filtering for viewers |
| `interchange` | Canonical JSON, hashes, export bundles, verified import |
| `federation` | Contracts only (remote source trait); see `docs/federation.md` |

## Quick start

```rust
use pardarsh_core::prelude::*;

let origin = Origin::new("records.example.org")?;
let mut repo = Repository::new(SqliteStore::open("records.sqlite", origin)?, Validator::default());
let me = Attribution::system("importer");
let decision = repo.create(RecordDraft::new(RecordKind::Decision, "Council approves ward budget"), Meta::by(me))?;
```

See `examples/legislative_change.rs` for a complete, non-issue consumer:

```bash
cargo run --example legislative_change
```

## Build and test

```bash
cargo test
```

```bash
cargo clippy --all-targets
```

Tests include store-agnostic repository tests run against both reference
stores, access/interchange tests, a boundary test asserting the crate never
references an application, and conformance fixtures under `fixtures/`.

## Documentation

* `docs/entities.md` — every model explained, with examples
* `docs/concepts.md` — glossary, principles, non-goals
* `docs/data-model.md` — envelope, kinds, relations, events, serialization
* `docs/provenance.md` — sources, authority, derivation, hashing
* `docs/versioning.md` — history model, concurrency, idempotency, schema compatibility
* `docs/federation.md` — what is in place and what is open
* `docs/adr/` — architecture decision records

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md). CI (`.github/workflows/ci.yml`) runs rustfmt, clippy, tests on Linux/Windows/macOS, the MSRV (1.85) build, docs, `cargo deny` and pre-commit hygiene checks.

## Status

Phase 1 foundation. Not implemented yet: federation transport and
signatures, JSON Schema generation from the Rust types, full-text search
(an application concern for now).

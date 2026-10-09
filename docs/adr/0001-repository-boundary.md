# ADR 0001: Two projects with a one-way dependency

Status: accepted (2026-10-09)

## Context
The brief separates reusable public-record infrastructure from the first
application (a civic issue ledger). Coupling them would make the
infrastructure issue-shaped and hard to reuse for laws, budgets or contracts.

## Decision
* `pardarsh-core` (this crate) owns identifiers, the envelope, extensions,
  events, provenance, relations, validation, storage interfaces, access
  primitives and interchange.
* Applications (the first is `loklekha`) live in separate projects and depend
  on `pardarsh-core` via a path dependency during early development, moving to
  a pinned git revision or versioned release as interfaces stabilise.
* `pardarsh-core` never imports or names an application. `tests/boundary.rs`
  fails the build if it does.
* Application vocabulary enters only through namespaced extensions, extension
  kinds/relations/roles, and event tags.

```text
loklekha (app: workflow, accounts, moderation, API, pages)
    |  uses Repository, Record, Event, Store, Viewer, Bundle ...
    v
pardarsh-core (model, schema, history, provenance, relations,
               validation, storage, graph, access, interchange)
```

## Consequences
A second consumer (`examples/legislative_change.rs`) uses the core with no
application code. Application statuses are extension fields, so the core has
no status vocabulary.

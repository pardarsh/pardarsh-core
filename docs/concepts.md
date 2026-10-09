# Concepts

## Principles

* **Connected, not centralized.** Records are owned by the instance (origin)
  that minted them. Shared identifiers, schemas and an interchange format let
  independent instances reference and exchange records without a shared database.
* **Authority is not truth.** An official source is evidence of what an
  institution published. Claims are attributed; evidence is linked with an
  explicit certainty; contested questions are not silently decided.
* **History is preserved.** Every change is an attributable event.
  Corrections, retractions, supersession and disputes are explicit. The only
  destructive operation is redaction of personal data (ADR 0006).
* **Status is a projection.** Current state is derived from history. Workflow
  states belong to applications (as extension fields), not to the core.
* **Unknown stays unknown.** Missing metadata is allowed; uncertain links are
  marked uncertain; remote references that cannot be resolved are reported as
  unresolved, never guessed.

## Glossary

| Term | Meaning |
| --- | --- |
| Origin | DNS-like authority of an instance; part of every record id |
| Record | Stable, typed unit of information with provenance, relations and history |
| Envelope | The fields every record has (see `data-model.md`) |
| Kind | The record type: issue, evidence, claim, proposal, comment, decision, action, project, outcome, actor, source, or a namespaced extension kind |
| Extension | Namespaced, typed fields added by an application |
| Event | Attributable, append-only change to one record |
| Projection | The current record state derived from its events |
| Relation | Typed, directed edge `source --kind--> target` stored on the source, with certainty |
| Certainty | `unverified`, `asserted`, `sourced` (cites sources), `disputed` |
| Provenance | Where a record or claim came from and how it was derived |
| Viewer | Who is reading; determines visible records, events, sources and identities |
| Bundle | Self-verifying export of records with their histories |
| Conformance | Fixtures any implementation of the format must accept or reject |

## Non-goals (for now)

* Deciding whether a claim is true or a law is valid.
* Application workflows, accounts, moderation policy or UI.
* A graph database. Relations are queried through the `Store` trait.
* Federation transport, signatures and discovery (contracts only).
* AI features. Nothing in the core requires machine-generated content; when
  it exists it must be marked with `Derivation::MachineGenerated`.

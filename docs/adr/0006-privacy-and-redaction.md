# ADR 0006: Visibility, withheld attribution and redaction

Status: accepted (2026-10-09)

## Decision
* Records, events and sources each carry a visibility (`public` or
  `restricted{audiences}`). The `access` module filters what a viewer sees;
  exports go through the same filter and are marked `filtered`.
* Attributions can be `withheld`: retained for accountability, shown as
  anonymous to everyone but the actor and privileged viewers.
* Redaction is the only destructive operation. A `redacted` event names field
  paths (title, summary, body, a source, an extension field, an annotation
  value); the store scrubs those values from the record *and from every
  earlier event* in the same transaction, replacing them with `[redacted]`,
  `"redacted"` or `urn:redacted:<source id>`. The redaction event itself
  (who, when, why, which paths) stays in history, and replaying the scrubbed
  history still yields the current record.

## Consequences
Bundles exported before a redaction still contain the original values;
propagating redactions to copies is a federation question (tombstones).
Authorization (who may redact, hide, or see what) is application policy.

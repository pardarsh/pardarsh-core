# Federation

Federation is a later capability. What exists now so that it is not blocked:

* Record ids include their origin, so remote records can be referenced directly.
* Relations accept remote targets and optional locators; traversal reports
  them as `unresolved` rather than guessing.
* Bundles are self-verifying (record and bundle hashes, history replay).
* Import enforces a policy: size and count limits, own-origin protection, an
  optional origin allow-list, full validation, divergence detection, and
  idempotent re-import (events keep their ids).
* `federation::RemoteSource` is the contract for fetching bundles.

Open questions: discovery and remote retrieval; push versus polling;
signatures and key rotation (hashes prove integrity, not authorship); replay
protection and cross-origin ordering; tombstones and propagating redactions;
trust and moderation across domains; schema negotiation; rate limits.
Standards to evaluate against concrete needs: ActivityPub, W3C PROV, OCDS,
JSON Schema, Linked Data.

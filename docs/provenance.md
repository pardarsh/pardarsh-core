# Provenance

A `SourceRef` describes an artifact a record relies on.

| Field | Required | Notes |
| --- | --- | --- |
| `id` | yes | Stable within the record; used for redaction |
| `locator` | yes | `https`, `http`, `urn` or `doi` URI. Other schemes (`javascript:`, `data:`, `file:`) are rejected |
| `title`, `media_type`, `issuer` | no | |
| `published_at` | no | Publication time claimed by or for the source |
| `retrieved_at` | no | When it was consulted; never conflated with publication |
| `content_hash` | no | `sha256:<hex>` of the exact retrieved bytes |
| `authority` | default `unknown` | `official`, `third_party`, `user_submitted`, `unknown` |
| `derivation` | default `original` | `original`, `extracted{method}`, `summary{author}`, `machine_generated{method, reviewed}` |
| `submitted_by` | no | Attribution of whoever attached the reference |
| `visibility` | default public | A source can be more restricted than its record |

Rules: only metadata that is known is recorded, and unknown is not an error.
User-supplied links are marked `user_submitted`. Machine-generated material
must say so and whether it was reviewed. An `official` source is evidence of
publication, not of truth.

**Hash semantics.** For sources: SHA-256 of the retrieved bytes. For records
and bundles: SHA-256 of canonical JSON (keys sorted by code point, no
insignificant whitespace, serde_json number and string formatting).

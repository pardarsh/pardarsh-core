//! Federation contracts (interfaces only; no network implementation yet).
//!
//! What is already in place to keep federation possible:
//! * identifiers carry their origin ([`crate::id::RecordId`]),
//! * relations may point at remote records and carry a `target_locator`,
//! * bundles are self-verifying and imports enforce an origin policy
//!   ([`crate::interchange::ImportPolicy`]),
//! * imported events keep their original ids, so re-sync is idempotent.
//!
//! Open questions (discovery, signatures, key rotation, replay protection,
//! tombstones, cross-domain moderation) are listed in `docs/federation.md`.

use crate::error::Result;
use crate::id::{Origin, RecordId};
use crate::interchange::Bundle;

/// Something that can fetch records published by another origin.
pub trait RemoteSource {
    fn origin(&self) -> &Origin;

    /// Fetch a bundle containing `ids` (and their histories). Implementations
    /// must treat the response as untrusted and pass it through
    /// [`crate::interchange::parse_bundle`] before use.
    fn fetch(&self, ids: &[RecordId]) -> Result<Bundle>;
}

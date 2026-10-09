//! Provenance: where a record or claim came from and how it was derived.
//!
//! A [`SourceRef`] points at an original artifact (a URL, a DOI, a URN) and
//! carries what is known about it. Unknown metadata is allowed: a report must
//! not be blocked because a publication date is missing. What *is* required is
//! honesty about authority and derivation: a user-submitted link is marked as
//! such, and machine-generated material is never presented as original.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::id::SourceId;
use crate::model::{Attribution, Timestamp, Visibility};

/// URI schemes accepted in source locators. Anything else (notably
/// `javascript:`, `data:` and `file:`) is rejected at validation.
pub const ALLOWED_LOCATOR_SCHEMES: &[&str] = &["https", "http", "urn", "doi"];

/// Who stands behind the source artifact.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceAuthority {
    /// Published or formally recorded by the issuing institution. This is
    /// evidence of what was published, not proof that its content is true.
    Official,
    /// Published by a third party (press, NGO, researcher...).
    ThirdParty,
    /// Supplied by a user of the system and not independently checked.
    UserSubmitted,
    #[default]
    Unknown,
}

/// How the material referenced here relates to the original artifact.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Derivation {
    /// The reference is to the original artifact itself.
    #[default]
    Original,
    /// Metadata or content extracted from the original by a described method.
    Extracted { method: String },
    /// A human-written summary or interpretation of the original.
    Summary { author: Option<Attribution> },
    /// Produced by an automated model; must be reviewable and correctable.
    MachineGenerated { method: String, reviewed: bool },
}

/// A reference to a source artifact with provenance metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceRef {
    pub id: SourceId,
    /// URI of the artifact (`https://...`, `urn:...`, `doi:...`).
    pub locator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    /// The institution or person that issued the artifact, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
    /// Publication time claimed by or for the source, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<Timestamp>,
    /// When the artifact was retrieved/consulted (distinct from publication).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieved_at: Option<Timestamp>,
    /// Hash of the retrieved bytes, if they were fetched. See [`ContentHash`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<ContentHash>,
    #[serde(default)]
    pub authority: SourceAuthority,
    #[serde(default)]
    pub derivation: Derivation,
    /// Who attached this reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_by: Option<Attribution>,
    /// A source can be more restricted than the record that cites it
    /// (for example a private attachment on a public issue).
    #[serde(default)]
    pub visibility: Visibility,
}

impl SourceRef {
    pub fn new(locator: impl Into<String>) -> Self {
        Self {
            id: SourceId::generate(),
            locator: locator.into(),
            title: None,
            media_type: None,
            issuer: None,
            published_at: None,
            retrieved_at: None,
            content_hash: None,
            authority: SourceAuthority::Unknown,
            derivation: Derivation::Original,
            submitted_by: None,
            visibility: Visibility::Public,
        }
    }

    pub fn title(mut self, t: impl Into<String>) -> Self {
        self.title = Some(t.into());
        self
    }

    pub fn issuer(mut self, i: impl Into<String>) -> Self {
        self.issuer = Some(i.into());
        self
    }

    pub fn authority(mut self, a: SourceAuthority) -> Self {
        self.authority = a;
        self
    }

    pub fn retrieved_at(mut self, t: Timestamp) -> Self {
        self.retrieved_at = Some(t);
        self
    }

    pub fn published_at(mut self, t: Timestamp) -> Self {
        self.published_at = Some(t);
        self
    }

    pub fn submitted_by(mut self, a: Attribution) -> Self {
        self.submitted_by = Some(a);
        self
    }

    pub fn visibility(mut self, v: Visibility) -> Self {
        self.visibility = v;
        self
    }

    pub fn derivation(mut self, d: Derivation) -> Self {
        self.derivation = d;
        self
    }

    /// The URI scheme of the locator, lowercased.
    pub fn scheme(&self) -> Option<String> {
        let (scheme, _) = self.locator.split_once(':')?;
        Some(scheme.to_ascii_lowercase())
    }
}

/// A content hash with an explicit algorithm prefix: `sha256:<64 hex>`.
///
/// Semantics: for source artifacts it is the hash of the exact retrieved
/// bytes; for records and bundles it is the hash of the canonical JSON
/// serialization defined in [`crate::interchange::canonical_json`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ContentHash(String);

impl ContentHash {
    pub fn sha256(bytes: &[u8]) -> Self {
        Self(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ContentHash {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.strip_prefix("sha256:") {
            Some(h) if h.len() == 64 && h.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) => {
                Ok(Self(s.to_string()))
            }
            _ => Err(format!("invalid content hash {s:?}: expected sha256:<64 lowercase hex>")),
        }
    }
}

impl Serialize for ContentHash {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

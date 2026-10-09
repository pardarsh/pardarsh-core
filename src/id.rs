//! Identifiers.
//!
//! Every record has a globally unambiguous [`RecordId`] of the form
//! `pd:<origin>:<uuid>`, where `<origin>` is the DNS-like authority of the
//! instance that minted it and `<uuid>` is a UUIDv7 (time-ordered). Because
//! the origin is part of the identifier, records from independently operated
//! instances can be referenced without coordination, which keeps federation
//! possible later without changing the identifier format.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// Prefix of every serialized record identifier.
pub const RECORD_ID_SCHEME: &str = "pd";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("invalid origin {0:?}: expected lowercase DNS-like name (a-z, 0-9, '-', '.')")]
    InvalidOrigin(String),
    #[error("invalid record id {0:?}: expected pd:<origin>:<uuid>")]
    InvalidRecordId(String),
    #[error("invalid namespaced name {0:?}: expected <namespace>:<name>")]
    InvalidName(String),
}

/// The authority (instance) that minted an identifier, e.g. `records.example.org`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Origin(String);

impl Origin {
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 253
            && value.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            });
        if valid { Ok(Self(value)) } else { Err(IdError::InvalidOrigin(value)) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Origin {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Origin::new(s)
    }
}

/// Stable, globally unambiguous record identifier: `pd:<origin>:<uuid>`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RecordId {
    origin: Origin,
    local: Uuid,
}

impl RecordId {
    /// Mint a new identifier at `origin`. UUIDv7 keeps ids roughly time-ordered.
    pub fn generate(origin: &Origin) -> Self {
        Self { origin: origin.clone(), local: Uuid::now_v7() }
    }

    pub fn from_parts(origin: Origin, local: Uuid) -> Self {
        Self { origin, local }
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    pub fn local(&self) -> Uuid {
        self.local
    }

    pub fn is_from(&self, origin: &Origin) -> bool {
        &self.origin == origin
    }
}

impl fmt::Display for RecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{RECORD_ID_SCHEME}:{}:{}", self.origin, self.local.hyphenated())
    }
}

impl FromStr for RecordId {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || IdError::InvalidRecordId(s.to_string());
        let rest = s.strip_prefix("pd:").ok_or_else(err)?;
        let (origin, local) = rest.rsplit_once(':').ok_or_else(err)?;
        let origin = Origin::new(origin).map_err(|_| err())?;
        // Only the canonical lowercase hyphenated form is accepted so that one
        // record cannot be referred to by several distinct strings.
        let local = Uuid::parse_str(local).map_err(|_| err())?;
        if local.hyphenated().to_string() != rest.rsplit_once(':').unwrap().1 {
            return Err(err());
        }
        Ok(Self { origin, local })
    }
}

impl Serialize for RecordId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for RecordId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl Serialize for Origin {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Origin {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Origin::new(s).map_err(serde::de::Error::custom)
    }
}

macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn generate() -> Self {
                Self(Uuid::now_v7())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self)
            }
        }
    };
}

uuid_id!(
    /// Identifier of a single event in a record's history.
    EventId
);
uuid_id!(
    /// Identifier of a relationship asserted on a record.
    RelationId
);
uuid_id!(
    /// Identifier of a source reference attached to a record.
    SourceId
);

/// A `namespace:name` pair used for extension record kinds, extension
/// relations, roles, event tags and annotations. Namespaces keep application
/// vocabulary (e.g. `app:status_change`) out of the core vocabulary.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NamespacedName {
    namespace: String,
    name: String,
}

impl NamespacedName {
    pub fn new(namespace: &str, name: &str) -> Result<Self, IdError> {
        let err = || IdError::InvalidName(format!("{namespace}:{name}"));
        if !is_namespace(namespace) || !is_simple_name(name) {
            return Err(err());
        }
        Ok(Self { namespace: namespace.to_string(), name: name.to_string() })
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// `[a-z][a-z0-9_.-]*`, at most 64 bytes.
pub fn is_namespace(s: &str) -> bool {
    let mut bytes = s.bytes();
    s.len() <= 64
        && matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-'))
}

/// `[a-z][a-z0-9_]*`, at most 64 bytes.
pub fn is_simple_name(s: &str) -> bool {
    let mut bytes = s.bytes();
    s.len() <= 64
        && matches!(bytes.next(), Some(b'a'..=b'z'))
        && bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

impl fmt::Display for NamespacedName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.name)
    }
}

impl FromStr for NamespacedName {
    type Err = IdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (ns, name) = s.split_once(':').ok_or_else(|| IdError::InvalidName(s.to_string()))?;
        NamespacedName::new(ns, name)
    }
}

impl Serialize for NamespacedName {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for NamespacedName {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Defines a vocabulary with a fixed set of core terms plus namespaced
/// extension terms, serialized as plain strings (`"issue"`, `"app:thing"`).
macro_rules! open_vocabulary {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident => $s:literal),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($(#[$vmeta])* $variant,)*
            /// A namespaced term defined outside the core vocabulary.
            Extension($crate::id::NamespacedName),
        }

        impl $name {
            /// The core (non-extension) terms of this vocabulary.
            pub const CORE: &'static [&'static str] = &[$($s),*];

            pub fn as_str(&self) -> std::borrow::Cow<'_, str> {
                match self {
                    $(Self::$variant => std::borrow::Cow::Borrowed($s),)*
                    Self::Extension(n) => std::borrow::Cow::Owned(n.to_string()),
                }
            }

            pub fn is_extension(&self) -> bool {
                matches!(self, Self::Extension(_))
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::id::IdError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($s => Ok(Self::$variant),)*
                    other => other.parse().map(Self::Extension),
                }
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = <String as serde::Deserialize>::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}
pub(crate) use open_vocabulary;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_id_round_trips() {
        let origin = Origin::new("ledger.example.org").unwrap();
        let id = RecordId::generate(&origin);
        let text = id.to_string();
        assert!(text.starts_with("pd:ledger.example.org:"));
        assert_eq!(text.parse::<RecordId>().unwrap(), id);
    }

    #[test]
    fn rejects_malformed_ids() {
        for bad in [
            "",
            "pd:",
            "pd:Example.org:0190a5d2-5b8e-7cc4-9a43-6e3d6f0c3c11",
            "xx:example.org:0190a5d2-5b8e-7cc4-9a43-6e3d6f0c3c11",
            "pd:example.org:not-a-uuid",
            "pd:example.org:0190A5D2-5B8E-7CC4-9A43-6E3D6F0C3C11",
            "pd:-bad.org:0190a5d2-5b8e-7cc4-9a43-6e3d6f0c3c11",
        ] {
            assert!(bad.parse::<RecordId>().is_err(), "{bad} should be rejected");
        }
    }

    #[test]
    fn namespaced_names() {
        let n: NamespacedName = "app:status_change".parse().unwrap();
        assert_eq!(n.namespace(), "app");
        assert_eq!(n.name(), "status_change");
        assert!("App:x".parse::<NamespacedName>().is_err());
        assert!("nocolon".parse::<NamespacedName>().is_err());
        assert!("a:b:c".parse::<NamespacedName>().is_err());
    }
}

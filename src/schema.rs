//! Schema versions and the compatibility policy.
//!
//! Versions are `MAJOR.MINOR`. A reader at `R` can read data written at `W`
//! when `W.major == R.major && W.minor <= R.minor`: minor versions only add
//! optional things, major versions may break. Data written by a *newer* minor
//! version is rejected rather than silently dropping fields it may carry.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SchemaVersion {
    pub major: u16,
    pub minor: u16,
}

/// Version of the common record envelope and event format defined by this crate.
pub const ENVELOPE_SCHEMA_VERSION: SchemaVersion = SchemaVersion::new(1, 0);

/// Version of the export bundle format.
pub const BUNDLE_FORMAT_VERSION: SchemaVersion = SchemaVersion::new(1, 0);

impl SchemaVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Can a reader that understands `self` read data written at `written`?
    pub fn can_read(&self, written: &SchemaVersion) -> bool {
        self.major == written.major && written.minor <= self.minor
    }
}

impl fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl FromStr for SchemaVersion {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (major, minor) = s.split_once('.').ok_or_else(|| format!("invalid schema version {s:?}"))?;
        let parse = |p: &str| {
            if p.is_empty() || (p.len() > 1 && p.starts_with('0')) {
                return Err(format!("invalid schema version {s:?}"));
            }
            p.parse::<u16>().map_err(|_| format!("invalid schema version {s:?}"))
        };
        Ok(Self { major: parse(major)?, minor: parse(minor)? })
    }
}

impl Serialize for SchemaVersion {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_policy() {
        let reader = SchemaVersion::new(1, 2);
        assert!(reader.can_read(&SchemaVersion::new(1, 0)));
        assert!(reader.can_read(&SchemaVersion::new(1, 2)));
        assert!(!reader.can_read(&SchemaVersion::new(1, 3)));
        assert!(!reader.can_read(&SchemaVersion::new(2, 0)));
        assert!(!reader.can_read(&SchemaVersion::new(0, 9)));
    }

    #[test]
    fn parses() {
        assert_eq!("1.0".parse::<SchemaVersion>().unwrap(), SchemaVersion::new(1, 0));
        assert!("1".parse::<SchemaVersion>().is_err());
        assert!("01.0".parse::<SchemaVersion>().is_err());
        assert!("1.x".parse::<SchemaVersion>().is_err());
    }
}

#![allow(dead_code)]

use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};
use pardarsh_core::prelude::*;

pub const NS: &str = "test.civic";

pub fn origin() -> Origin {
    Origin::new("local.test").unwrap()
}

pub fn registry() -> ExtensionRegistry {
    let mut r = ExtensionRegistry::new();
    r.register(
        ExtensionSchema::new(NS, SchemaVersion::new(1, 0))
            .applies_to([RecordKind::Issue])
            .field(
                "category",
                FieldSpec::required(FieldType::Enum { values: vec!["roads".into(), "water".into()] }, "category"),
            )
            .field("locality", FieldSpec::optional(FieldType::Text { max_len: 50 }, "locality"))
            .field("reporter_phone", FieldSpec::optional(FieldType::Text { max_len: 20 }, "personal data"))
            .field("owner", FieldSpec::optional(FieldType::Reference { kinds: vec![RecordKind::Actor] }, "owner"))
            .field("severity", FieldSpec::optional(FieldType::Integer { min: Some(1), max: Some(5) }, "severity")),
    )
    .unwrap();
    r
}

pub fn clock() -> ManualClock {
    ManualClock::new(Utc.with_ymd_and_hms(2026, 1, 15, 10, 0, 0).unwrap())
}

pub fn memory_repo() -> Repository<MemoryStore> {
    Repository::new(MemoryStore::new(origin()), Validator::new(registry())).with_clock(clock())
}

pub fn sqlite_repo() -> Repository<SqliteStore> {
    Repository::new(SqliteStore::open_in_memory(origin()).unwrap(), Validator::new(registry())).with_clock(clock())
}

pub fn alice() -> Attribution {
    Attribution::public(ActorRef::System { name: "alice".into() })
}

pub fn ext(category: &str) -> BTreeMap<String, ExtValue> {
    BTreeMap::from([("category".to_string(), ExtValue::Enum(category.into()))])
}

pub fn issue_draft(title: &str) -> RecordDraft {
    RecordDraft::new(RecordKind::Issue, title).extension(NS, SchemaVersion::new(1, 0), ext("roads"))
}

/// Run a generic test function against every reference store.
#[macro_export]
macro_rules! for_each_store {
    ($($f:ident),* $(,)?) => {
        $(
            mod $f {
                #[test]
                fn memory() {
                    super::$f(&mut super::common::memory_repo());
                }
                #[test]
                fn sqlite() {
                    super::$f(&mut super::common::sqlite_repo());
                }
            }
        )*
    };
}

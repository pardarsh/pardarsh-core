//! Conformance fixtures: any implementation of the record format should
//! accept everything under `fixtures/**/valid` and `fixtures/bundles`, and
//! reject everything under `fixtures/**/invalid`.

use std::fs;
use std::path::Path;

use pardarsh_core::interchange::{self, ImportPolicy};
use pardarsh_core::prelude::*;

fn json_files(dir: &str) -> Vec<(String, Vec<u8>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
    let mut out: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), fs::read(&p).unwrap()))
        .collect();
    out.sort();
    assert!(!out.is_empty(), "no fixtures in {}", dir.display());
    out
}

#[test]
fn valid_records() {
    for (name, bytes) in json_files("fixtures/records/valid") {
        let record: Record = serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        Validator::default().validate_record(&record).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn invalid_records() {
    for (name, bytes) in json_files("fixtures/records/invalid") {
        let rejected = match serde_json::from_slice::<Record>(&bytes) {
            Err(_) => true,
            Ok(record) => Validator::default().validate_record(&record).is_err(),
        };
        assert!(rejected, "{name} should be rejected");
    }
}

#[test]
fn bundles_verify_and_import() {
    for (name, bytes) in json_files("fixtures/bundles") {
        let bundle =
            interchange::parse_bundle(&bytes, &ImportPolicy::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut repo =
            Repository::new(MemoryStore::new(Origin::new("conformance.test").unwrap()), Validator::default());
        let report =
            interchange::import(&mut repo, &bundle, &ImportPolicy::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(report.records, bundle.entries.len());
        for entry in &bundle.entries {
            assert_eq!(repo.get(&entry.record.id).unwrap(), entry.record, "{name}");
        }
    }
}

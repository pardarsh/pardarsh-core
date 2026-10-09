//! The infrastructure must not depend on, or know about, any application.

use std::fs;
use std::path::Path;

fn scan(dir: &Path, hits: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            scan(&path, hits);
        } else if path.extension().is_some_and(|e| e == "rs" || e == "toml") {
            let text = fs::read_to_string(&path).unwrap().to_lowercase();
            for app in ["loklekha", "civicledger", "civic_ledger"] {
                if text.contains(app) {
                    hits.push(format!("{} mentions {app}", path.display()));
                }
            }
        }
    }
}

#[test]
fn core_does_not_reference_applications() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut hits = Vec::new();
    scan(&root.join("src"), &mut hits);
    scan(&root.join("examples"), &mut hits);
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap().to_lowercase();
    if manifest.contains("loklekha") {
        hits.push("Cargo.toml depends on loklekha".into());
    }
    assert!(hits.is_empty(), "{hits:#?}");
}

//! Canonical serialization, content hashing, export and import.
//!
//! The interchange format is a JSON [`Bundle`]: records with their full (or
//! viewer-filtered) histories. Hashes are SHA-256 over the *canonical JSON*
//! form: UTF-8, object keys sorted by code point, no insignificant
//! whitespace, numbers and strings as produced by `serde_json`.
//!
//! Imported bundles are untrusted: size limits, format/version checks, hash
//! verification, origin policy, history replay and full validation are all
//! applied before anything is written.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::access::{Viewer, view_history, view_record};
use crate::error::{Error, Result};
use crate::event::Event;
use crate::id::{Origin, RecordId};
use crate::model::{Record, Timestamp};
use crate::projection::project;
use crate::provenance::ContentHash;
use crate::repository::Repository;
use crate::schema::{BUNDLE_FORMAT_VERSION, ENVELOPE_SCHEMA_VERSION, SchemaVersion};
use crate::storage::Store;

pub const BUNDLE_FORMAT: &str = "pardarsh.bundle";

/// Serialize to canonical JSON bytes.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let v = serde_json::to_value(value)?;
    let mut out = Vec::new();
    write_canonical(&v, &mut out)?;
    Ok(out)
}

fn write_canonical(v: &Value, out: &mut Vec<u8>) -> Result<()> {
    match v {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push(b'{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                serde_json::to_writer(&mut *out, k)?;
                out.push(b':');
                write_canonical(&map[k], out)?;
            }
            out.push(b'}');
        }
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_canonical(item, out)?;
            }
            out.push(b']');
        }
        scalar => serde_json::to_writer(&mut *out, scalar)?,
    }
    Ok(())
}

/// SHA-256 of the canonical JSON form.
pub fn content_hash<T: Serialize>(value: &T) -> Result<ContentHash> {
    Ok(ContentHash::sha256(&canonical_json(value)?))
}

/// Whether a bundle carries complete histories or a viewer-filtered view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    /// Full histories; records can be re-derived and the bundle imported.
    Complete,
    /// Restricted events/sources removed for a viewer; for reading only.
    Filtered,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BundleEntry {
    pub record: Record,
    pub history: Vec<Event>,
    pub record_hash: ContentHash,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bundle {
    pub format: String,
    pub format_version: SchemaVersion,
    pub envelope_version: SchemaVersion,
    pub origin: Origin,
    pub exported_at: Timestamp,
    pub completeness: Completeness,
    pub entries: Vec<BundleEntry>,
    /// Hash of the canonical bundle with this field set to `null`.
    #[serde(default)]
    pub bundle_hash: Option<ContentHash>,
}

impl Bundle {
    fn compute_hash(&self) -> Result<ContentHash> {
        let mut copy = self.clone();
        copy.bundle_hash = None;
        content_hash(&copy)
    }

    pub fn to_json_pretty(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

/// Export records (and their histories) as `viewer` may see them. Records
/// the viewer cannot see are silently omitted.
pub fn export<S: Store>(repo: &Repository<S>, ids: &[RecordId], viewer: &Viewer) -> Result<Bundle> {
    let mut entries = Vec::new();
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(record) = repo.find(id)? else { continue };
        let Some(record) = view_record(&record, viewer) else { continue };
        let history = view_history(&repo.history(id)?, viewer);
        let record_hash = content_hash(&record)?;
        entries.push(BundleEntry { record, history, record_hash });
    }
    let mut bundle = Bundle {
        format: BUNDLE_FORMAT.to_string(),
        format_version: BUNDLE_FORMAT_VERSION,
        envelope_version: ENVELOPE_SCHEMA_VERSION,
        origin: repo.store().origin().clone(),
        exported_at: repo.now(),
        completeness: if viewer.privileged { Completeness::Complete } else { Completeness::Filtered },
        entries,
        bundle_hash: None,
    };
    bundle.bundle_hash = Some(bundle.compute_hash()?);
    Ok(bundle)
}

/// Rules for accepting a bundle from elsewhere.
#[derive(Clone, Debug)]
pub struct ImportPolicy {
    pub max_bytes: usize,
    pub max_records: usize,
    pub max_events_per_record: usize,
    /// Accept records minted by the importing store's own origin
    /// (e.g. restoring a backup). Off by default: a remote party must not be
    /// able to inject records that look locally authored.
    pub allow_own_origin: bool,
    /// If set, only records from these origins are accepted.
    pub trusted_origins: Option<BTreeSet<Origin>>,
}

impl Default for ImportPolicy {
    fn default() -> Self {
        Self {
            max_bytes: 16 * 1024 * 1024,
            max_records: 10_000,
            max_events_per_record: 10_000,
            allow_own_origin: false,
            trusted_origins: None,
        }
    }
}

/// Parse and verify an untrusted bundle without writing anything.
pub fn parse_bundle(bytes: &[u8], policy: &ImportPolicy) -> Result<Bundle> {
    if bytes.len() > policy.max_bytes {
        return Err(Error::Import(format!("bundle is {} bytes; limit is {}", bytes.len(), policy.max_bytes)));
    }
    let bundle: Bundle = serde_json::from_slice(bytes).map_err(|e| Error::Import(format!("malformed bundle: {e}")))?;
    verify_bundle(&bundle, policy)?;
    Ok(bundle)
}

pub fn verify_bundle(bundle: &Bundle, policy: &ImportPolicy) -> Result<()> {
    let err = |m: String| Err(Error::Import(m));
    if bundle.format != BUNDLE_FORMAT {
        return err(format!("unknown format {:?}", bundle.format));
    }
    if !BUNDLE_FORMAT_VERSION.can_read(&bundle.format_version) {
        return err(format!("unsupported bundle version {}", bundle.format_version));
    }
    if !ENVELOPE_SCHEMA_VERSION.can_read(&bundle.envelope_version) {
        return err(format!("unsupported envelope version {}", bundle.envelope_version));
    }
    if bundle.entries.len() > policy.max_records {
        return err(format!("{} records exceed the limit of {}", bundle.entries.len(), policy.max_records));
    }
    match &bundle.bundle_hash {
        Some(h) if *h == bundle.compute_hash()? => {}
        Some(_) => return err("bundle hash does not match its contents".into()),
        None => return err("bundle hash is missing".into()),
    }
    for entry in &bundle.entries {
        let id = &entry.record.id;
        if entry.history.len() > policy.max_events_per_record {
            return err(format!("{id}: too many events"));
        }
        if content_hash(&entry.record)? != entry.record_hash {
            return err(format!("{id}: record hash does not match"));
        }
        if entry.history.iter().any(|e| &e.record_id != id) {
            return err(format!("{id}: history contains events of another record"));
        }
        if bundle.completeness == Completeness::Complete {
            let projected = project(&entry.history).map_err(|e| Error::Import(format!("{id}: {e}")))?;
            if projected.as_ref() != Some(&entry.record) {
                return err(format!("{id}: record does not match its history"));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub records: usize,
    pub events_written: usize,
    pub events_already_present: usize,
}

/// Import a verified, complete bundle. Events already present (same event id
/// at the same position) are skipped, so re-importing is idempotent and a
/// newer export of the same records only appends what is new.
pub fn import<S: Store>(repo: &mut Repository<S>, bundle: &Bundle, policy: &ImportPolicy) -> Result<ImportReport> {
    verify_bundle(bundle, policy)?;
    if bundle.completeness != Completeness::Complete {
        return Err(Error::Import("filtered bundles cannot be imported".into()));
    }
    let own = repo.store().origin().clone();
    for entry in &bundle.entries {
        let origin = entry.record.id.origin();
        if origin == &own && !policy.allow_own_origin {
            return Err(Error::Import(format!("{}: records of the local origin are not accepted", entry.record.id)));
        }
        if let Some(trusted) = &policy.trusted_origins {
            if !trusted.contains(origin) {
                return Err(Error::Import(format!("{}: origin {origin} is not trusted", entry.record.id)));
            }
        }
    }
    let mut report = ImportReport::default();
    for entry in &bundle.entries {
        let existing = repo.history(&entry.record.id)?;
        for (i, local) in existing.iter().enumerate() {
            if entry.history.get(i).map(|e| e.id) != Some(local.id) {
                return Err(Error::Import(format!(
                    "{}: local history diverges from the bundle at sequence {}",
                    entry.record.id, local.sequence
                )));
            }
        }
        report.events_already_present += existing.len();
        for event in entry.history.iter().skip(existing.len()) {
            repo.ingest(event.clone())?;
            report.events_written += 1;
        }
        report.records += 1;
    }
    Ok(report)
}

//! Structural and semantic validation.
//!
//! The repository validates the *resulting state* of every command, so the
//! same rules apply to creation, edits, imports and replays. Checks that need
//! other records (reference targets) are done by the repository.

use std::collections::HashSet;
use std::fmt;

use chrono::Duration;
use serde::{Deserialize, Serialize};

use crate::event::{Event, EventKind};
use crate::extension::ExtensionRegistry;
use crate::model::{Record, Timestamp};
use crate::provenance::{ALLOWED_LOCATOR_SCHEMES, Derivation, SourceRef};
use crate::relation::Certainty;
use crate::schema::ENVELOPE_SCHEMA_VERSION;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub path: String,
    pub message: String,
}

impl ValidationIssue {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self { path: path.into(), message: message.into() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub struct ValidationErrors {
    pub issues: Vec<ValidationIssue>,
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "validation failed: ")?;
        for (i, issue) in self.issues.iter().enumerate() {
            if i > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{}: {}", issue.path, issue.message)?;
        }
        Ok(())
    }
}

impl ValidationErrors {
    pub fn single(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self { issues: vec![ValidationIssue::new(path, message)] }
    }

    fn check(issues: Vec<ValidationIssue>) -> Result<(), Self> {
        if issues.is_empty() { Ok(()) } else { Err(Self { issues }) }
    }
}

/// Size limits applied to every record. Generous for legitimate use, small
/// enough to bound the cost of untrusted input.
#[derive(Clone, Debug)]
pub struct Limits {
    pub title_chars: usize,
    pub summary_chars: usize,
    pub body_chars: usize,
    pub reason_chars: usize,
    pub locator_bytes: usize,
    pub max_sources: usize,
    pub max_relations: usize,
    pub max_actors: usize,
    pub max_idempotency_key: usize,
    /// How far in the future an `occurred_at` may be (clock skew allowance).
    pub max_clock_skew: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            title_chars: 300,
            summary_chars: 2_000,
            body_chars: 50_000,
            reason_chars: 2_000,
            locator_bytes: 2_048,
            max_sources: 100,
            max_relations: 1_000,
            max_actors: 50,
            max_idempotency_key: 128,
            max_clock_skew: Duration::minutes(5),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Validator {
    pub registry: ExtensionRegistry,
    pub limits: Limits,
}

impl Validator {
    pub fn new(registry: ExtensionRegistry) -> Self {
        Self { registry, limits: Limits::default() }
    }

    pub fn validate_record(&self, record: &Record) -> Result<(), ValidationErrors> {
        let l = &self.limits;
        let mut issues = Vec::new();
        if !ENVELOPE_SCHEMA_VERSION.can_read(&record.schema_version) {
            issues.push(ValidationIssue::new(
                "schema_version",
                format!(
                    "envelope version {} is not supported (reader {})",
                    record.schema_version, ENVELOPE_SCHEMA_VERSION
                ),
            ));
        }
        if record.title.trim().is_empty() {
            issues.push(ValidationIssue::new("title", "must not be empty"));
        }
        text(&mut issues, "title", Some(&record.title), l.title_chars, false);
        text(&mut issues, "summary", record.summary.as_deref(), l.summary_chars, true);
        text(&mut issues, "body", record.body.as_deref(), l.body_chars, true);
        if let (Some(from), Some(until)) = (record.times.effective_from, record.times.effective_until) {
            if from > until {
                issues.push(ValidationIssue::new("times.effective_until", "is before effective_from"));
            }
        }
        if record.actors.len() > l.max_actors {
            issues.push(ValidationIssue::new("actors", format!("more than {} actors", l.max_actors)));
        }
        if record.sources.len() > l.max_sources {
            issues.push(ValidationIssue::new("sources", format!("more than {} sources", l.max_sources)));
        }
        let mut seen = HashSet::new();
        for (i, s) in record.sources.iter().enumerate() {
            if !seen.insert(s.id) {
                issues.push(ValidationIssue::new(format!("sources[{i}].id"), "duplicate source id"));
            }
            self.validate_source(s, &format!("sources[{i}]"), &mut issues);
        }
        if record.relations.len() > l.max_relations {
            issues.push(ValidationIssue::new("relations", format!("more than {} relations", l.max_relations)));
        }
        let mut seen = HashSet::new();
        for (i, r) in record.relations.iter().enumerate() {
            let p = format!("relations[{i}]");
            if !seen.insert(r.id) {
                issues.push(ValidationIssue::new(format!("{p}.id"), "duplicate relation id"));
            }
            if r.target == record.id {
                issues.push(ValidationIssue::new(format!("{p}.target"), "a record cannot relate to itself"));
            }
            if r.certainty == Certainty::Sourced && r.sources.is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{p}.certainty"),
                    "sourced relations must cite at least one source",
                ));
            }
            if let Some(loc) = &r.target_locator {
                check_locator(loc, &format!("{p}.target_locator"), l.locator_bytes, &mut issues);
            }
            text(&mut issues, &format!("{p}.note"), r.note.as_deref(), l.summary_chars, true);
            for (j, s) in r.sources.iter().enumerate() {
                self.validate_source(s, &format!("{p}.sources[{j}]"), &mut issues);
            }
        }
        self.registry.validate(record, &mut issues);
        ValidationErrors::check(issues)
    }

    fn validate_source(&self, s: &SourceRef, path: &str, issues: &mut Vec<ValidationIssue>) {
        // Redacted locators use the reserved urn:redacted: form.
        if !s.locator.starts_with("urn:redacted:") {
            check_locator(&s.locator, &format!("{path}.locator"), self.limits.locator_bytes, issues);
        }
        text(issues, &format!("{path}.title"), s.title.as_deref(), self.limits.title_chars, false);
        text(issues, &format!("{path}.issuer"), s.issuer.as_deref(), self.limits.title_chars, false);
        if let Derivation::Extracted { method } | Derivation::MachineGenerated { method, .. } = &s.derivation {
            if method.trim().is_empty() {
                issues.push(ValidationIssue::new(format!("{path}.derivation.method"), "must describe the method"));
            }
        }
    }

    /// Checks on event metadata that do not depend on the record state.
    pub fn validate_event(&self, event: &Event, now: Timestamp) -> Result<(), ValidationErrors> {
        let l = &self.limits;
        let mut issues = Vec::new();
        if event.occurred_at > now + l.max_clock_skew {
            issues.push(ValidationIssue::new("occurred_at", "is in the future"));
        }
        if event.occurred_at > event.recorded_at + l.max_clock_skew {
            issues.push(ValidationIssue::new("occurred_at", "is after recorded_at"));
        }
        text(&mut issues, "reason", event.reason.as_deref(), l.reason_chars, true);
        let needs_reason = matches!(
            event.kind,
            EventKind::Corrected { .. }
                | EventKind::Retracted
                | EventKind::Redacted { .. }
                | EventKind::Superseded { .. }
        );
        if needs_reason && event.reason.as_deref().is_none_or(|r| r.trim().is_empty()) {
            issues.push(ValidationIssue::new("reason", format!("a reason is required for {}", event.kind.name())));
        }
        if let Some(key) = &event.idempotency_key {
            if key.is_empty()
                || key.len() > l.max_idempotency_key
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
            {
                issues.push(ValidationIssue::new("idempotency_key", "must be 1-128 characters of [A-Za-z0-9-_.:]"));
            }
        }
        match &event.kind {
            EventKind::Amended { changes } | EventKind::Corrected { changes, .. } if changes.is_empty() => {
                issues.push(ValidationIssue::new("changes", "must not be empty"));
            }
            EventKind::Redacted { fields } if fields.is_empty() => {
                issues.push(ValidationIssue::new("fields", "must not be empty"));
            }
            _ => {}
        }
        ValidationErrors::check(issues)
    }
}

fn text(issues: &mut Vec<ValidationIssue>, path: &str, value: Option<&str>, max: usize, multiline: bool) {
    let Some(v) = value else { return };
    if v.chars().count() > max {
        issues.push(ValidationIssue::new(path, format!("longer than {max} characters")));
    }
    if v.chars().any(|c| c.is_control() && !(multiline && (c == '\n' || c == '\r' || c == '\t'))) {
        issues.push(ValidationIssue::new(path, "contains control characters"));
    }
}

fn check_locator(loc: &str, path: &str, max: usize, issues: &mut Vec<ValidationIssue>) {
    if loc.len() > max {
        issues.push(ValidationIssue::new(path, format!("longer than {max} bytes")));
    }
    if loc.chars().any(|c| c.is_whitespace() || c.is_control()) {
        issues.push(ValidationIssue::new(path, "must not contain whitespace"));
    }
    let scheme = loc.split_once(':').map(|(s, _)| s.to_ascii_lowercase());
    match scheme {
        Some(s) if ALLOWED_LOCATOR_SCHEMES.contains(&s.as_str()) => {
            let rest = &loc[s.len() + 1..];
            if (s == "http" || s == "https") && !rest.starts_with("//") || rest.len() < 3 {
                issues.push(ValidationIssue::new(path, "malformed locator"));
            }
        }
        _ => issues
            .push(ValidationIssue::new(path, format!("scheme must be one of {}", ALLOWED_LOCATOR_SCHEMES.join(", ")))),
    }
}

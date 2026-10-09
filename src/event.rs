//! Append-only, attributable record history.
//!
//! Every change to a record is an [`Event`] with a per-record, gap-free
//! `sequence` (1, 2, 3...). The current [`crate::model::Record`] is a
//! projection of those events. Events are never edited or deleted, with one
//! exception: [`EventKind::Redacted`] scrubs the redacted values out of
//! earlier events so personal data can actually be removed (ADR 0006).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::extension::ExtValue;
use crate::id::{EventId, NamespacedName, RecordId, RelationId, SourceId};
use crate::model::{ActorRole, Attribution, Record, Timestamp, Visibility};
use crate::provenance::SourceRef;
use crate::relation::Relation;

/// Placeholder that replaces redacted text.
pub const REDACTED: &str = "[redacted]";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub record_id: RecordId,
    /// Position in the record's history, starting at 1, without gaps.
    pub sequence: u64,
    pub kind: EventKind,
    /// Who caused the event.
    pub attribution: Attribution,
    /// When the change happened according to the actor (may be earlier than
    /// `recorded_at`, never later than it plus allowed clock skew).
    pub occurred_at: Timestamp,
    /// When this system recorded the event. Set by the repository.
    pub recorded_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Application-defined label, e.g. `app:status_change`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<NamespacedName>,
    /// Who may see this event (e.g. private moderation notes).
    #[serde(default)]
    pub visibility: Visibility,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    /// The record's initial state.
    Created {
        record: Box<Record>,
    },
    /// Ordinary edits.
    Amended {
        changes: Vec<Change>,
    },
    /// Edits that fix an error. A reason is required; the event being
    /// corrected may be named.
    Corrected {
        changes: Vec<Change>,
        corrects: Option<EventId>,
    },
    RelationAdded {
        relation: Relation,
    },
    RelationRetracted {
        relation_id: RelationId,
    },
    /// The record was withdrawn. Reason required.
    Retracted,
    /// The record was replaced by another record.
    Superseded {
        by: RecordId,
    },
    /// Values at the given paths were removed from the record and its history.
    Redacted {
        fields: Vec<FieldPath>,
    },
    /// An application-defined annotation that does not change the envelope
    /// (for example a moderation note). Data is typed.
    Annotated {
        name: NamespacedName,
        data: BTreeMap<String, ExtValue>,
    },
}

impl EventKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Created { .. } => "created",
            Self::Amended { .. } => "amended",
            Self::Corrected { .. } => "corrected",
            Self::RelationAdded { .. } => "relation_added",
            Self::RelationRetracted { .. } => "relation_retracted",
            Self::Retracted => "retracted",
            Self::Superseded { .. } => "superseded",
            Self::Redacted { .. } => "redacted",
            Self::Annotated { .. } => "annotated",
        }
    }
}

/// A single field-level change.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "field", content = "value", rename_all = "snake_case")]
pub enum Change {
    Title(String),
    Summary(Option<String>),
    Body(Option<String>),
    PublishedAt(Option<Timestamp>),
    EffectiveFrom(Option<Timestamp>),
    EffectiveUntil(Option<Timestamp>),
    ObservedAt(Option<Timestamp>),
    Visibility(Visibility),
    AddActor(ActorRole),
    AddSource(SourceRef),
    SetExtensionField { namespace: String, field: String, value: Option<ExtValue> },
}

impl Change {
    /// The path this change writes to, if it writes a redactable value.
    pub fn path(&self) -> Option<FieldPath> {
        match self {
            Self::Title(_) => Some(FieldPath::Title),
            Self::Summary(_) => Some(FieldPath::Summary),
            Self::Body(_) => Some(FieldPath::Body),
            Self::AddSource(s) => Some(FieldPath::Source { id: s.id }),
            Self::SetExtensionField { namespace, field, .. } => {
                Some(FieldPath::Extension { namespace: namespace.clone(), field: field.clone() })
            }
            _ => None,
        }
    }
}

/// An addressable, redactable value in a record.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "path", rename_all = "snake_case")]
pub enum FieldPath {
    Title,
    Summary,
    Body,
    /// A source reference's locator and descriptive metadata.
    Source {
        id: SourceId,
    },
    Extension {
        namespace: String,
        field: String,
    },
    /// Free-text in an annotation's data.
    Annotation {
        event: EventId,
        key: String,
    },
}

/// Remove the values at `paths` from a record in place.
pub fn redact_record(record: &mut Record, paths: &[FieldPath]) {
    for path in paths {
        match path {
            FieldPath::Title => record.title = REDACTED.to_string(),
            FieldPath::Summary => {
                if record.summary.is_some() {
                    record.summary = Some(REDACTED.to_string())
                }
            }
            FieldPath::Body => {
                if record.body.is_some() {
                    record.body = Some(REDACTED.to_string())
                }
            }
            FieldPath::Source { id } => {
                for s in record.sources.iter_mut().filter(|s| s.id == *id) {
                    redact_source(s);
                }
            }
            FieldPath::Extension { namespace, field } => {
                if let Some(v) = record.extensions.get_mut(namespace).and_then(|e| e.fields.get_mut(field)) {
                    *v = ExtValue::Redacted;
                }
            }
            FieldPath::Annotation { .. } => {}
        }
    }
}

fn redact_source(s: &mut SourceRef) {
    s.locator = format!("urn:redacted:{}", s.id);
    s.title = None;
    s.issuer = None;
    s.content_hash = None;
    s.media_type = None;
}

/// Scrub redacted values out of an earlier event (used by stores when a
/// [`EventKind::Redacted`] event is committed).
pub fn scrub_event(event: &mut Event, paths: &[FieldPath]) {
    let redact_changes = |changes: &mut Vec<Change>| {
        for change in changes.iter_mut() {
            let Some(p) = change.path() else { continue };
            if !paths.contains(&p) {
                continue;
            }
            match change {
                Change::Title(t) => *t = REDACTED.to_string(),
                Change::Summary(s) | Change::Body(s) => {
                    if s.is_some() {
                        *s = Some(REDACTED.to_string())
                    }
                }
                Change::AddSource(s) => redact_source(s),
                Change::SetExtensionField { value: value @ Some(_), .. } => *value = Some(ExtValue::Redacted),
                _ => {}
            }
        }
    };
    match &mut event.kind {
        EventKind::Created { record } => redact_record(record, paths),
        EventKind::Amended { changes } | EventKind::Corrected { changes, .. } => redact_changes(changes),
        EventKind::Annotated { data, .. } => {
            for p in paths {
                if let FieldPath::Annotation { event: eid, key } = p {
                    if *eid == event.id {
                        if let Some(v) = data.get_mut(key) {
                            *v = ExtValue::Redacted;
                        }
                    }
                }
            }
        }
        EventKind::RelationAdded { relation } => {
            for s in relation.sources.iter_mut() {
                if paths.contains(&FieldPath::Source { id: s.id }) {
                    redact_source(s);
                }
            }
        }
        _ => {}
    }
}

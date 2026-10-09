//! Deriving current state from history, and explaining history to humans.

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::event::{Change, Event, EventKind, redact_record};
use crate::id::NamespacedName;
use crate::model::{Attribution, Lifecycle, Record, Timestamp, Visibility};

/// Apply one event to the current state.
pub fn apply(state: Option<Record>, event: &Event) -> Result<Record, Error> {
    let fail = |m: String| Error::Projection(format!("event {} (seq {}): {m}", event.id, event.sequence));
    let mut record = match (&event.kind, state) {
        (EventKind::Created { record }, None) => {
            if event.sequence != 1 {
                return Err(fail("created event must have sequence 1".into()));
            }
            if record.id != event.record_id {
                return Err(fail("created snapshot id does not match event record id".into()));
            }
            (**record).clone()
        }
        (EventKind::Created { .. }, Some(_)) => return Err(fail("record already exists".into())),
        (_, None) => return Err(fail("first event must be created".into())),
        (kind, Some(mut record)) => {
            if record.id != event.record_id {
                return Err(fail("event belongs to another record".into()));
            }
            if event.sequence != record.version + 1 {
                return Err(fail(format!("expected sequence {}", record.version + 1)));
            }
            match kind {
                EventKind::Created { .. } => unreachable!(),
                EventKind::Amended { changes } | EventKind::Corrected { changes, .. } => {
                    for change in changes {
                        apply_change(&mut record, change).map_err(fail)?;
                    }
                }
                EventKind::RelationAdded { relation } => {
                    if record.relations.iter().any(|r| r.id == relation.id) {
                        return Err(fail(format!("relation {} already exists", relation.id)));
                    }
                    record.relations.push(relation.clone());
                }
                EventKind::RelationRetracted { relation_id } => {
                    let rel = record
                        .relations
                        .iter_mut()
                        .find(|r| r.id == *relation_id && !r.retracted)
                        .ok_or_else(|| fail(format!("no active relation {relation_id}")))?;
                    rel.retracted = true;
                }
                EventKind::Retracted => record.lifecycle = Lifecycle::Retracted,
                EventKind::Superseded { by } => record.lifecycle = Lifecycle::Superseded { by: by.clone() },
                EventKind::Redacted { fields } => {
                    redact_record(&mut record, fields);
                    for f in fields {
                        if !record.redactions.contains(f) {
                            record.redactions.push(f.clone());
                        }
                    }
                }
                EventKind::Annotated { .. } => {}
            }
            record
        }
    };
    record.version = event.sequence;
    record.updated_at = event.recorded_at;
    Ok(record)
}

fn apply_change(record: &mut Record, change: &Change) -> Result<(), String> {
    match change {
        Change::Title(t) => record.title = t.clone(),
        Change::Summary(s) => record.summary = s.clone(),
        Change::Body(b) => record.body = b.clone(),
        Change::PublishedAt(t) => record.times.published_at = *t,
        Change::EffectiveFrom(t) => record.times.effective_from = *t,
        Change::EffectiveUntil(t) => record.times.effective_until = *t,
        Change::ObservedAt(t) => record.times.observed_at = *t,
        Change::Visibility(v) => record.visibility = v.clone(),
        Change::AddActor(a) => record.actors.push(a.clone()),
        Change::AddSource(s) => {
            if record.sources.iter().any(|x| x.id == s.id) {
                return Err(format!("source {} already attached", s.id));
            }
            record.sources.push(s.clone())
        }
        Change::SetExtensionField { namespace, field, value } => {
            let ext =
                record.extensions.get_mut(namespace).ok_or_else(|| format!("record has no extension {namespace}"))?;
            match value {
                Some(v) => {
                    ext.fields.insert(field.clone(), v.clone());
                }
                None => {
                    ext.fields.remove(field);
                }
            }
        }
    }
    Ok(())
}

/// Replay a complete history. Returns `None` for an empty history.
pub fn project(events: &[Event]) -> Result<Option<Record>, Error> {
    let mut state = None;
    for e in events {
        state = Some(apply(state, e)?);
    }
    Ok(state)
}

/// One human-readable field difference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldDiff {
    pub field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// Field-level differences between two states of a record.
pub fn diff(before: Option<&Record>, after: &Record) -> Vec<FieldDiff> {
    let mut out = Vec::new();
    let mut push = |field: String, b: Option<String>, a: Option<String>| {
        if b != a {
            out.push(FieldDiff { field, before: b, after: a });
        }
    };
    let ts = |t: Option<Timestamp>| t.map(|t| t.to_rfc3339());
    push("title".into(), before.map(|r| r.title.clone()), Some(after.title.clone()));
    push("summary".into(), before.and_then(|r| r.summary.clone()), after.summary.clone());
    push("body".into(), before.and_then(|r| r.body.clone()), after.body.clone());
    push("published_at".into(), before.and_then(|r| ts(r.times.published_at)), ts(after.times.published_at));
    push("effective_from".into(), before.and_then(|r| ts(r.times.effective_from)), ts(after.times.effective_from));
    push("effective_until".into(), before.and_then(|r| ts(r.times.effective_until)), ts(after.times.effective_until));
    push("observed_at".into(), before.and_then(|r| ts(r.times.observed_at)), ts(after.times.observed_at));
    push(
        "visibility".into(),
        before.map(|r| visibility_label(&r.visibility)),
        Some(visibility_label(&after.visibility)),
    );
    if before.is_some() || after.lifecycle != Lifecycle::Active {
        push(
            "lifecycle".into(),
            before.map(|r| lifecycle_label(&r.lifecycle)),
            Some(lifecycle_label(&after.lifecycle)),
        );
    }

    let mut namespaces: Vec<&String> = after.extensions.keys().collect();
    if let Some(b) = before {
        namespaces.extend(b.extensions.keys());
    }
    namespaces.sort();
    namespaces.dedup();
    for ns in namespaces {
        let b = before.and_then(|r| r.extensions.get(ns));
        let a = after.extensions.get(ns);
        let mut fields: Vec<&String> = a.map(|a| a.fields.keys().collect()).unwrap_or_default();
        if let Some(b) = b {
            fields.extend(b.fields.keys());
        }
        fields.sort();
        fields.dedup();
        for f in fields {
            push(
                format!("{ns}.{f}"),
                b.and_then(|e| e.fields.get(f)).map(|v| v.display()),
                a.and_then(|e| e.fields.get(f)).map(|v| v.display()),
            );
        }
    }

    for s in &after.sources {
        if !before.is_some_and(|b| b.sources.iter().any(|x| x.id == s.id)) {
            push("sources".into(), None, Some(s.locator.clone()));
        }
    }
    for r in &after.relations {
        let prev = before.and_then(|b| b.relations.iter().find(|x| x.id == r.id));
        match prev {
            None => push("relations".into(), None, Some(format!("{} {}", r.kind, r.target))),
            Some(p) if !p.retracted && r.retracted => {
                push("relations".into(), Some(format!("{} {}", r.kind, r.target)), Some("retracted".into()))
            }
            _ => {}
        }
    }
    let before_actors = before.map_or(0, |b| b.actors.len());
    for a in after.actors.iter().skip(before_actors) {
        push("actors".into(), None, Some(a.role.to_string()));
    }
    out
}

fn visibility_label(v: &Visibility) -> String {
    match v {
        Visibility::Public => "public".into(),
        Visibility::Restricted { audiences } if audiences.is_empty() => "private".into(),
        Visibility::Restricted { audiences } => {
            format!("restricted({})", audiences.iter().cloned().collect::<Vec<_>>().join(","))
        }
    }
}

fn lifecycle_label(l: &Lifecycle) -> String {
    match l {
        Lifecycle::Active => "active".into(),
        Lifecycle::Retracted => "retracted".into(),
        Lifecycle::Superseded { by } => format!("superseded by {by}"),
    }
}

/// A readable history entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub sequence: u64,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<NamespacedName>,
    pub attribution: Attribution,
    pub occurred_at: Timestamp,
    pub recorded_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub changes: Vec<FieldDiff>,
}

/// Replay a history and describe each step. Works on filtered histories
/// (gaps are tolerated) by diffing only the events present.
pub fn timeline(events: &[Event]) -> Vec<TimelineEntry> {
    let mut state: Option<Record> = None;
    let mut out = Vec::with_capacity(events.len());
    for e in events {
        let next = match (&e.kind, &state) {
            (EventKind::Created { record }, _) => Some((**record).clone()),
            (_, Some(s)) => {
                // Tolerate gaps from filtered histories by forcing the sequence.
                let mut s = s.clone();
                s.version = e.sequence - 1;
                apply(Some(s), e).ok()
            }
            (_, None) => None,
        };
        let changes = match (&next, &e.kind) {
            (_, EventKind::Annotated { data, .. }) => data
                .iter()
                .map(|(k, v)| FieldDiff { field: k.clone(), before: None, after: Some(v.display()) })
                .collect(),
            (Some(n), _) => diff(state.as_ref(), n),
            (None, _) => Vec::new(),
        };
        out.push(TimelineEntry {
            sequence: e.sequence,
            event: e.kind.name().to_string(),
            tag: e.tag.clone(),
            attribution: e.attribution.clone(),
            occurred_at: e.occurred_at,
            recorded_at: e.recorded_at,
            reason: e.reason.clone(),
            changes,
        });
        if next.is_some() {
            state = next;
        }
    }
    out
}

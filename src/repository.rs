//! The command side: every change goes through here.
//!
//! The repository turns a command into an [`Event`], projects the next state,
//! validates it (structure, extensions, relation rules, reference targets),
//! and commits it with optimistic concurrency and idempotency. It does *not*
//! decide who may do what: authorization is the caller's responsibility
//! (see [`crate::access`] for the visibility primitives).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use chrono::{DateTime, Utc};

use crate::error::{Error, Result};
use crate::event::{Change, Event, EventKind, FieldPath};
use crate::extension::{ExtValue, extension_references};
use crate::id::{EventId, NamespacedName, RecordId, RelationId};
use crate::model::{Attribution, Lifecycle, Record, RecordDraft, RecordTimes, Timestamp, Visibility};
use crate::projection::{TimelineEntry, apply, timeline};
use crate::relation::{Relation, RelationDraft};
use crate::schema::ENVELOPE_SCHEMA_VERSION;
use crate::storage::{IncomingEdge, RecordQuery, Store};
use crate::validation::{ValidationErrors, ValidationIssue, Validator};

/// Source of "now". Injected so tests are deterministic.
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        Utc::now()
    }
}

/// A manually advanced clock for tests and fixtures.
#[derive(Debug, Clone)]
pub struct ManualClock(Arc<AtomicI64>);

impl ManualClock {
    pub fn new(start: Timestamp) -> Self {
        Self(Arc::new(AtomicI64::new(start.timestamp_millis())))
    }

    pub fn advance(&self, d: chrono::Duration) {
        self.0.fetch_add(d.num_milliseconds(), Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        DateTime::from_timestamp_millis(self.0.load(Ordering::SeqCst)).expect("valid timestamp")
    }
}

/// Metadata common to every command.
#[derive(Clone, Debug)]
pub struct Meta {
    pub attribution: Attribution,
    /// When it happened according to the actor; defaults to now.
    pub occurred_at: Option<Timestamp>,
    /// Retrying a command with the same key returns the original result.
    pub idempotency_key: Option<String>,
    /// Reject the command unless the record is at this version.
    pub expected_version: Option<u64>,
    pub reason: Option<String>,
    pub tag: Option<NamespacedName>,
    pub visibility: Visibility,
}

impl Meta {
    pub fn by(attribution: Attribution) -> Self {
        Self {
            attribution,
            occurred_at: None,
            idempotency_key: None,
            expected_version: None,
            reason: None,
            tag: None,
            visibility: Visibility::Public,
        }
    }

    pub fn reason(mut self, r: impl Into<String>) -> Self {
        self.reason = Some(r.into());
        self
    }

    pub fn idempotency_key(mut self, k: impl Into<String>) -> Self {
        self.idempotency_key = Some(k.into());
        self
    }

    pub fn expect_version(mut self, v: u64) -> Self {
        self.expected_version = Some(v);
        self
    }

    pub fn tag(mut self, t: NamespacedName) -> Self {
        self.tag = Some(t);
        self
    }

    pub fn occurred_at(mut self, t: Timestamp) -> Self {
        self.occurred_at = Some(t);
        self
    }

    pub fn visibility(mut self, v: Visibility) -> Self {
        self.visibility = v;
        self
    }
}

/// Result of a command.
#[derive(Clone, Debug)]
pub struct Committed {
    pub record: Record,
    pub event: Event,
    /// True when the idempotency key matched an earlier identical command and
    /// nothing new was written.
    pub replayed: bool,
}

pub struct Repository<S: Store> {
    store: S,
    validator: Validator,
    clock: Box<dyn Clock>,
}

impl<S: Store> Repository<S> {
    pub fn new(store: S, validator: Validator) -> Self {
        Self { store, validator, clock: Box::new(SystemClock) }
    }

    pub fn with_clock(mut self, clock: impl Clock + 'static) -> Self {
        self.clock = Box::new(clock);
        self
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn validator(&self) -> &Validator {
        &self.validator
    }

    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }

    // ---- queries -------------------------------------------------------

    pub fn get(&self, id: &RecordId) -> Result<Record> {
        self.store.get(id)?.ok_or_else(|| Error::NotFound(id.clone()))
    }

    pub fn find(&self, id: &RecordId) -> Result<Option<Record>> {
        self.store.get(id)
    }

    pub fn history(&self, id: &RecordId) -> Result<Vec<Event>> {
        self.store.history(id)
    }

    pub fn timeline(&self, id: &RecordId) -> Result<Vec<TimelineEntry>> {
        Ok(timeline(&self.store.history(id)?))
    }

    pub fn list(&self, query: &RecordQuery) -> Result<Vec<Record>> {
        self.store.list(query)
    }

    pub fn incoming(&self, id: &RecordId) -> Result<Vec<IncomingEdge>> {
        self.store.incoming(id)
    }

    // ---- commands ------------------------------------------------------

    pub fn create(&mut self, draft: RecordDraft, meta: Meta) -> Result<Committed> {
        if let Some(c) = self.replay(&meta, None)? {
            return Ok(c);
        }
        let now = self.clock.now();
        let id = RecordId::generate(self.store.origin());
        let relations = draft.relations.into_iter().map(|r| r.into_relation(meta.attribution.clone(), now)).collect();
        let record = Record {
            id: id.clone(),
            kind: draft.kind,
            schema_version: ENVELOPE_SCHEMA_VERSION,
            title: draft.title,
            summary: draft.summary,
            body: draft.body,
            times: RecordTimes {
                recorded_at: now,
                published_at: draft.published_at,
                effective_from: draft.effective_from,
                effective_until: draft.effective_until,
                observed_at: draft.observed_at,
            },
            actors: draft.actors,
            sources: draft.sources,
            relations,
            visibility: draft.visibility,
            lifecycle: Lifecycle::Active,
            extensions: draft.extensions,
            redactions: Vec::new(),
            version: 1,
            updated_at: now,
        };
        self.execute(id, EventKind::Created { record: Box::new(record) }, meta, None)
    }

    pub fn amend(&mut self, id: &RecordId, changes: Vec<Change>, meta: Meta) -> Result<Committed> {
        self.run(id, EventKind::Amended { changes }, meta)
    }

    pub fn correct(
        &mut self,
        id: &RecordId,
        changes: Vec<Change>,
        corrects: Option<EventId>,
        meta: Meta,
    ) -> Result<Committed> {
        if let Some(target) = corrects {
            if !self.store.history(id)?.iter().any(|e| e.id == target) {
                return Err(ValidationErrors::single("corrects", "event is not part of this record's history").into());
            }
        }
        self.run(id, EventKind::Corrected { changes, corrects }, meta)
    }

    pub fn set_extension_field(
        &mut self,
        id: &RecordId,
        namespace: &str,
        field: &str,
        value: Option<ExtValue>,
        meta: Meta,
    ) -> Result<Committed> {
        let change = Change::SetExtensionField { namespace: namespace.into(), field: field.into(), value };
        self.amend(id, vec![change], meta)
    }

    pub fn link(&mut self, source: &RecordId, draft: RelationDraft, meta: Meta) -> Result<Committed> {
        let now = self.clock.now();
        let relation = draft.into_relation(meta.attribution.clone(), now);
        self.run(source, EventKind::RelationAdded { relation }, meta)
    }

    pub fn unlink(&mut self, source: &RecordId, relation_id: RelationId, meta: Meta) -> Result<Committed> {
        self.run(source, EventKind::RelationRetracted { relation_id }, meta)
    }

    pub fn retract(&mut self, id: &RecordId, meta: Meta) -> Result<Committed> {
        self.run(id, EventKind::Retracted, meta)
    }

    pub fn supersede(&mut self, id: &RecordId, by: &RecordId, meta: Meta) -> Result<Committed> {
        let replacement = self.get(by)?;
        let current = self.get(id)?;
        if replacement.kind != current.kind {
            return Err(
                ValidationErrors::single("by", "a record can only be superseded by a record of the same kind").into()
            );
        }
        self.run(id, EventKind::Superseded { by: by.clone() }, meta)
    }

    pub fn redact(&mut self, id: &RecordId, fields: Vec<FieldPath>, meta: Meta) -> Result<Committed> {
        self.run(id, EventKind::Redacted { fields }, meta)
    }

    pub fn annotate(
        &mut self,
        id: &RecordId,
        name: NamespacedName,
        data: BTreeMap<String, ExtValue>,
        meta: Meta,
    ) -> Result<Committed> {
        self.run(id, EventKind::Annotated { name, data }, meta)
    }

    /// Commit already-formed events received from elsewhere (import). The
    /// events keep their ids and times; they are re-projected and
    /// validated exactly like local commands.
    pub fn ingest(&mut self, event: Event) -> Result<Record> {
        let current = self.store.get(&event.record_id)?;
        let next = apply(current.clone(), &event)?;
        self.validator.validate_event(&event, event.recorded_at)?;
        self.validator.validate_record(&next)?;
        self.store.commit(event, next.clone())?;
        Ok(next)
    }

    // ---- internals -----------------------------------------------------

    fn run(&mut self, id: &RecordId, kind: EventKind, meta: Meta) -> Result<Committed> {
        if let Some(c) = self.replay(&meta, Some((id, &kind)))? {
            return Ok(c);
        }
        let current = self.get(id)?;
        if let Some(expected) = meta.expected_version {
            if expected != current.version {
                return Err(Error::Conflict { id: id.clone(), expected, actual: current.version });
            }
        }
        if current.lifecycle == Lifecycle::Retracted
            && !matches!(kind, EventKind::Redacted { .. } | EventKind::Annotated { .. } | EventKind::Corrected { .. })
        {
            return Err(Error::NotPermitted(format!(
                "{id} is retracted; only corrections, redactions and annotations are allowed"
            )));
        }
        if matches!(kind, EventKind::Retracted) && current.lifecycle == Lifecycle::Retracted {
            return Err(Error::NotPermitted(format!("{id} is already retracted")));
        }
        self.execute(id.clone(), kind, meta, Some(current))
    }

    /// If the idempotency key was used before, return the original result
    /// when the command is the same, or fail when it is different.
    fn replay(&self, meta: &Meta, target: Option<(&RecordId, &EventKind)>) -> Result<Option<Committed>> {
        let Some(key) = &meta.idempotency_key else { return Ok(None) };
        let Some(previous) = self.store.event_by_idempotency_key(key)? else { return Ok(None) };
        let same = match (target, &previous.kind) {
            (None, EventKind::Created { .. }) => true,
            (Some((id, kind)), prev) => &previous.record_id == id && same_command(kind, prev),
            _ => false,
        };
        if !same || previous.attribution != meta.attribution {
            return Err(Error::IdempotencyConflict(key.clone()));
        }
        let record = self.get(&previous.record_id)?;
        Ok(Some(Committed { record, event: previous, replayed: true }))
    }

    fn execute(&mut self, id: RecordId, kind: EventKind, meta: Meta, current: Option<Record>) -> Result<Committed> {
        let now = match &kind {
            // Keep the snapshot's recorded time and the event's identical.
            EventKind::Created { record } => record.times.recorded_at,
            _ => self.clock.now(),
        };
        let event = Event {
            id: EventId::generate(),
            record_id: id.clone(),
            sequence: current.as_ref().map_or(1, |r| r.version + 1),
            kind,
            attribution: meta.attribution,
            occurred_at: meta.occurred_at.unwrap_or(now),
            recorded_at: now,
            idempotency_key: meta.idempotency_key,
            reason: meta.reason,
            tag: meta.tag,
            visibility: meta.visibility,
        };
        self.validator.validate_event(&event, now)?;
        let next = apply(current.clone(), &event)?;
        self.validator.validate_record(&next)?;
        self.check_references(current.as_ref(), &next)?;
        self.store.commit(event.clone(), next.clone())?;
        Ok(Committed { record: next, event, replayed: false })
    }

    /// Local reference targets must exist and satisfy relation/field kind
    /// rules. Remote targets cannot be checked here and are accepted.
    fn check_references(&self, before: Option<&Record>, next: &Record) -> Result<()> {
        let origin = self.store.origin();
        let mut issues = Vec::new();
        let is_new = |r: &Relation| before.is_none_or(|b| !b.relations.iter().any(|x| x.id == r.id));
        for (i, rel) in next.relations.iter().enumerate().filter(|(_, r)| is_new(r)) {
            let path = format!("relations[{i}]");
            if !rel.target.is_from(origin) {
                continue;
            }
            let Some(target) = self.store.get(&rel.target)? else {
                issues.push(ValidationIssue::new(format!("{path}.target"), "target record does not exist"));
                continue;
            };
            if let Some(kinds) = rel.kind.target_kinds() {
                if !kinds.contains(&target.kind) {
                    issues.push(ValidationIssue::new(
                        format!("{path}.target"),
                        format!(
                            "{} requires a target of kind {:?}, got {}",
                            rel.kind,
                            kinds.iter().map(|k| k.to_string()).collect::<Vec<_>>(),
                            target.kind
                        ),
                    ));
                }
            }
            if rel.kind.requires_same_kind() && target.kind != next.kind {
                issues.push(ValidationIssue::new(
                    format!("{path}.target"),
                    format!("{} requires records of the same kind", rel.kind),
                ));
            }
            let duplicate = next
                .relations
                .iter()
                .any(|r| r.id != rel.id && !r.retracted && r.kind == rel.kind && r.target == rel.target);
            if duplicate {
                issues.push(ValidationIssue::new(path, "an identical active relation already exists"));
            }
        }
        for (path, target, kinds) in extension_references(&self.validator.registry, next) {
            if !target.is_from(origin) {
                continue;
            }
            match self.store.get(target)? {
                None => issues.push(ValidationIssue::new(path, "referenced record does not exist")),
                Some(t) if !kinds.is_empty() && !kinds.contains(&t.kind) => {
                    issues.push(ValidationIssue::new(path, format!("referenced record has kind {}", t.kind)))
                }
                Some(_) => {}
            }
        }
        if issues.is_empty() { Ok(()) } else { Err(ValidationErrors { issues }.into()) }
    }
}

/// Two commands are "the same" for idempotency if their payloads match,
/// ignoring ids and times generated while handling them.
fn same_command(a: &EventKind, b: &EventKind) -> bool {
    fn normalize(v: serde_json::Value) -> serde_json::Value {
        match v {
            serde_json::Value::Object(map) => map
                .into_iter()
                .filter(|(k, _)| !matches!(k.as_str(), "id" | "asserted_at"))
                .map(|(k, v)| (k, normalize(v)))
                .collect(),
            serde_json::Value::Array(items) => items.into_iter().map(normalize).collect(),
            other => other,
        }
    }
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(x), Ok(y)) => normalize(x) == normalize(y),
        _ => false,
    }
}

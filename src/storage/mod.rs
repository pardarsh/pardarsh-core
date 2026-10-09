//! Storage interface and reference implementations.
//!
//! Stores persist events and the projected snapshot of each record. They are
//! deliberately dumb: validation, authorization and projection happen in the
//! [`crate::repository::Repository`]. A store's only semantic duties are
//! atomicity, optimistic concurrency (gap-free sequences), idempotency-key
//! uniqueness, and scrubbing history when a redaction is committed.

mod memory;
#[cfg(feature = "sqlite")]
mod sqlite;

pub use memory::MemoryStore;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteStore;

use crate::error::Result;
use crate::event::Event;
use crate::id::{Origin, RecordId};
use crate::model::{Lifecycle, Record, RecordKind};
use crate::relation::Relation;

/// Filter for listing records. Text search is an application concern.
#[derive(Clone, Debug, Default)]
pub struct RecordQuery {
    /// Only these kinds (empty = all kinds).
    pub kinds: Vec<RecordKind>,
    /// Only records carrying this extension namespace.
    pub extension: Option<String>,
    /// Include retracted and superseded records.
    pub include_inactive: bool,
    pub limit: Option<usize>,
    pub offset: usize,
}

impl RecordQuery {
    pub fn kind(kind: RecordKind) -> Self {
        Self { kinds: vec![kind], ..Self::default() }
    }

    pub(crate) fn matches(&self, r: &Record) -> bool {
        (self.kinds.is_empty() || self.kinds.contains(&r.kind))
            && self.extension.as_ref().is_none_or(|ns| r.extensions.contains_key(ns))
            && (self.include_inactive || r.lifecycle == Lifecycle::Active)
    }

    pub(crate) fn page<I: Iterator<Item = Record>>(&self, it: I) -> Vec<Record> {
        let it = it.filter(|r| self.matches(r)).skip(self.offset);
        match self.limit {
            Some(n) => it.take(n).collect(),
            None => it.collect(),
        }
    }
}

/// An edge pointing *at* a record, as seen from the target.
#[derive(Clone, Debug, PartialEq)]
pub struct IncomingEdge {
    pub source: RecordId,
    pub source_kind: RecordKind,
    pub relation: Relation,
}

pub trait Store: Send {
    /// The authority this store mints identifiers for.
    fn origin(&self) -> &Origin;

    /// Current snapshot of a record.
    fn get(&self, id: &RecordId) -> Result<Option<Record>>;

    /// Full history of a record, ordered by sequence.
    fn history(&self, id: &RecordId) -> Result<Vec<Event>>;

    fn event_by_idempotency_key(&self, key: &str) -> Result<Option<Event>>;

    /// Atomically append `event` and replace the snapshot with `snapshot`.
    ///
    /// Must fail with [`crate::Error::Conflict`] unless `event.sequence` is
    /// exactly one more than the stored version, and with
    /// [`crate::Error::IdempotencyConflict`] if the event's idempotency key is
    /// already used. When `event` is a redaction, earlier events of the record
    /// must be scrubbed with [`crate::event::scrub_event`] in the same
    /// transaction.
    fn commit(&mut self, event: Event, snapshot: Record) -> Result<()>;

    /// Records matching the query, newest first.
    fn list(&self, query: &RecordQuery) -> Result<Vec<Record>>;

    /// Relations (including retracted ones) whose target is `target`.
    fn incoming(&self, target: &RecordId) -> Result<Vec<IncomingEdge>>;
}

pub(crate) fn check_sequence(id: &RecordId, current: u64, event: &Event, snapshot: &Record) -> Result<()> {
    if event.sequence != current + 1 || snapshot.version != event.sequence || &snapshot.id != id {
        return Err(crate::Error::Conflict {
            id: id.clone(),
            expected: event.sequence.saturating_sub(1),
            actual: current,
        });
    }
    Ok(())
}

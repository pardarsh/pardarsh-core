use std::collections::{BTreeMap, HashMap};

use super::{IncomingEdge, RecordQuery, Store, check_sequence};
use crate::error::{Error, Result};
use crate::event::{Event, EventKind, scrub_event};
use crate::id::{Origin, RecordId};
use crate::model::Record;

/// In-memory reference store. Useful for tests, examples and tooling.
#[derive(Debug)]
pub struct MemoryStore {
    origin: Origin,
    records: BTreeMap<RecordId, (Record, Vec<Event>)>,
    idempotency: HashMap<String, (RecordId, usize)>,
}

impl MemoryStore {
    pub fn new(origin: Origin) -> Self {
        Self { origin, records: BTreeMap::new(), idempotency: HashMap::new() }
    }
}

impl Store for MemoryStore {
    fn origin(&self) -> &Origin {
        &self.origin
    }

    fn get(&self, id: &RecordId) -> Result<Option<Record>> {
        Ok(self.records.get(id).map(|(r, _)| r.clone()))
    }

    fn history(&self, id: &RecordId) -> Result<Vec<Event>> {
        Ok(self.records.get(id).map(|(_, e)| e.clone()).unwrap_or_default())
    }

    fn event_by_idempotency_key(&self, key: &str) -> Result<Option<Event>> {
        Ok(self.idempotency.get(key).and_then(|(id, i)| self.records.get(id).map(|(_, events)| events[*i].clone())))
    }

    fn commit(&mut self, event: Event, snapshot: Record) -> Result<()> {
        let id = event.record_id.clone();
        let current = self.records.get(&id).map_or(0, |(r, _)| r.version);
        check_sequence(&id, current, &event, &snapshot)?;
        if let Some(key) = &event.idempotency_key {
            if self.idempotency.contains_key(key) {
                return Err(Error::IdempotencyConflict(key.clone()));
            }
        }
        let key = event.idempotency_key.clone();
        let entry = self.records.entry(id.clone()).or_insert_with(|| (snapshot.clone(), Vec::new()));
        if let EventKind::Redacted { fields } = &event.kind {
            for e in entry.1.iter_mut() {
                scrub_event(e, fields);
            }
        }
        entry.0 = snapshot;
        entry.1.push(event);
        if let Some(key) = key {
            self.idempotency.insert(key, (id, entry.1.len() - 1));
        }
        Ok(())
    }

    fn list(&self, query: &RecordQuery) -> Result<Vec<Record>> {
        Ok(query.page(self.records.values().rev().map(|(r, _)| r.clone())))
    }

    fn incoming(&self, target: &RecordId) -> Result<Vec<IncomingEdge>> {
        let mut out = Vec::new();
        for (record, _) in self.records.values() {
            for rel in record.relations.iter().filter(|r| &r.target == target) {
                out.push(IncomingEdge {
                    source: record.id.clone(),
                    source_kind: record.kind.clone(),
                    relation: rel.clone(),
                });
            }
        }
        Ok(out)
    }
}

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::{IncomingEdge, RecordQuery, Store, check_sequence};
use crate::error::{Error, Result};
use crate::event::{Event, EventKind, scrub_event};
use crate::id::{Origin, RecordId};
use crate::model::Record;

/// Relational reference store (SQLite). Events and snapshots are stored as
/// canonical JSON next to a few indexed columns used for lookups. The schema
/// is internal to this store; applications must not depend on it.
pub struct SqliteStore {
    origin: Origin,
    conn: Connection,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS pd_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS pd_records (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    version INTEGER NOT NULL,
    json TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS pd_records_kind ON pd_records(kind);
CREATE TABLE IF NOT EXISTS pd_events (
    record_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    event_id TEXT NOT NULL UNIQUE,
    idempotency_key TEXT UNIQUE,
    json TEXT NOT NULL,
    PRIMARY KEY (record_id, sequence)
);
CREATE TABLE IF NOT EXISTS pd_relations (
    source TEXT NOT NULL,
    relation_id TEXT NOT NULL,
    target TEXT NOT NULL,
    PRIMARY KEY (source, relation_id)
);
CREATE INDEX IF NOT EXISTS pd_relations_target ON pd_relations(target);
"#;

fn db(e: rusqlite::Error) -> Error {
    Error::Storage(e.to_string())
}

impl SqliteStore {
    pub fn open(path: impl AsRef<Path>, origin: Origin) -> Result<Self> {
        Self::init(Connection::open(path).map_err(db)?, origin)
    }

    pub fn open_in_memory(origin: Origin) -> Result<Self> {
        Self::init(Connection::open_in_memory().map_err(db)?, origin)
    }

    fn init(conn: Connection, origin: Origin) -> Result<Self> {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;").map_err(db)?;
        conn.execute_batch(SCHEMA).map_err(db)?;
        // A database belongs to one origin; refuse to reopen it under another.
        let existing: Option<String> = conn
            .query_row("SELECT value FROM pd_meta WHERE key = 'origin'", [], |r| r.get(0))
            .optional()
            .map_err(db)?;
        match existing {
            Some(o) if o != origin.as_str() => {
                return Err(Error::Storage(format!("database belongs to origin {o}, not {origin}")));
            }
            Some(_) => {}
            None => {
                conn.execute("INSERT INTO pd_meta (key, value) VALUES ('origin', ?1)", [origin.as_str()])
                    .map_err(db)?;
            }
        }
        Ok(Self { origin, conn })
    }

    fn decode<T: serde::de::DeserializeOwned>(json: String) -> Result<T> {
        serde_json::from_str(&json).map_err(|e| Error::Storage(format!("corrupt row: {e}")))
    }
}

impl Store for SqliteStore {
    fn origin(&self) -> &Origin {
        &self.origin
    }

    fn get(&self, id: &RecordId) -> Result<Option<Record>> {
        let json: Option<String> = self
            .conn
            .query_row("SELECT json FROM pd_records WHERE id = ?1", [id.to_string()], |r| r.get(0))
            .optional()
            .map_err(db)?;
        json.map(Self::decode).transpose()
    }

    fn history(&self, id: &RecordId) -> Result<Vec<Event>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT json FROM pd_events WHERE record_id = ?1 ORDER BY sequence")
            .map_err(db)?;
        let rows = stmt.query_map([id.to_string()], |r| r.get::<_, String>(0)).map_err(db)?;
        rows.map(|r| r.map_err(db).and_then(Self::decode)).collect()
    }

    fn event_by_idempotency_key(&self, key: &str) -> Result<Option<Event>> {
        let json: Option<String> = self
            .conn
            .query_row("SELECT json FROM pd_events WHERE idempotency_key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(db)?;
        json.map(Self::decode).transpose()
    }

    fn commit(&mut self, event: Event, snapshot: Record) -> Result<()> {
        let id = event.record_id.to_string();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(db)?;
        let current: u64 = tx
            .query_row("SELECT version FROM pd_records WHERE id = ?1", [&id], |r| r.get(0))
            .optional()
            .map_err(db)?
            .unwrap_or(0);
        check_sequence(&event.record_id, current, &event, &snapshot)?;
        if let Some(key) = &event.idempotency_key {
            let used: Option<i64> = tx
                .query_row("SELECT 1 FROM pd_events WHERE idempotency_key = ?1", [key], |r| r.get(0))
                .optional()
                .map_err(db)?;
            if used.is_some() {
                return Err(Error::IdempotencyConflict(key.clone()));
            }
        }
        if let EventKind::Redacted { fields } = &event.kind {
            let earlier: Vec<(i64, String)> = {
                let mut stmt = tx.prepare("SELECT sequence, json FROM pd_events WHERE record_id = ?1").map_err(db)?;
                let rows = stmt.query_map([&id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(db)?;
                rows.collect::<std::result::Result<_, _>>().map_err(db)?
            };
            for (seq, json) in earlier {
                let mut e: Event = Self::decode(json)?;
                scrub_event(&mut e, fields);
                tx.execute(
                    "UPDATE pd_events SET json = ?1 WHERE record_id = ?2 AND sequence = ?3",
                    params![serde_json::to_string(&e)?, &id, seq],
                )
                .map_err(db)?;
            }
        }
        tx.execute(
            "INSERT INTO pd_events (record_id, sequence, event_id, idempotency_key, json) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&id, event.sequence, event.id.to_string(), event.idempotency_key, serde_json::to_string(&event)?],
        )
        .map_err(db)?;
        tx.execute(
            "INSERT INTO pd_records (id, kind, version, json) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET version = excluded.version, json = excluded.json",
            params![&id, snapshot.kind.to_string(), snapshot.version, serde_json::to_string(&snapshot)?],
        )
        .map_err(db)?;
        tx.execute("DELETE FROM pd_relations WHERE source = ?1", [&id]).map_err(db)?;
        for rel in &snapshot.relations {
            tx.execute(
                "INSERT INTO pd_relations (source, relation_id, target) VALUES (?1, ?2, ?3)",
                params![&id, rel.id.to_string(), rel.target.to_string()],
            )
            .map_err(db)?;
        }
        tx.commit().map_err(db)
    }

    fn list(&self, query: &RecordQuery) -> Result<Vec<Record>> {
        let mut stmt = self.conn.prepare_cached("SELECT kind, json FROM pd_records ORDER BY id DESC").map_err(db)?;
        let kinds: Vec<String> = query.kinds.iter().map(|k| k.to_string()).collect();
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).map_err(db)?;
        let mut records = Vec::new();
        for row in rows {
            let (kind, json) = row.map_err(db)?;
            if kinds.is_empty() || kinds.contains(&kind) {
                records.push(Self::decode(json)?);
            }
        }
        Ok(query.page(records.into_iter()))
    }

    fn incoming(&self, target: &RecordId) -> Result<Vec<IncomingEdge>> {
        let mut stmt =
            self.conn.prepare_cached("SELECT DISTINCT source FROM pd_relations WHERE target = ?1").map_err(db)?;
        let sources: Vec<String> = stmt
            .query_map([target.to_string()], |r| r.get(0))
            .map_err(db)?
            .collect::<std::result::Result<_, _>>()
            .map_err(db)?;
        let mut out = Vec::new();
        for s in sources {
            let id: RecordId = s.parse()?;
            if let Some(record) = self.get(&id)? {
                for rel in record.relations.iter().filter(|r| &r.target == target) {
                    out.push(IncomingEdge {
                        source: record.id.clone(),
                        source_kind: record.kind.clone(),
                        relation: rel.clone(),
                    });
                }
            }
        }
        Ok(out)
    }
}

//! Relationship queries and bounded traversal.

use std::collections::{HashSet, VecDeque};

use serde::Serialize;

use crate::error::Result;
use crate::id::RecordId;
use crate::model::RecordKind;
use crate::relation::{Relation, RelationKind};
use crate::storage::Store;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}

/// An edge `source --relation.kind--> relation.target`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Edge {
    pub source: RecordId,
    pub relation: Relation,
}

#[derive(Clone, Debug)]
pub struct TraversalOptions {
    pub direction: Direction,
    /// Only follow these relation kinds (empty = all).
    pub kinds: Vec<RelationKind>,
    pub max_depth: usize,
    /// Hard cap on visited records, to bound work on large graphs.
    pub max_nodes: usize,
    pub include_retracted: bool,
}

impl Default for TraversalOptions {
    fn default() -> Self {
        Self { direction: Direction::Both, kinds: Vec::new(), max_depth: 2, max_nodes: 500, include_retracted: false }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Traversal {
    /// Visited records with their distance from the start.
    pub nodes: Vec<(RecordId, usize)>,
    pub edges: Vec<Edge>,
    /// Records referenced by an edge but not present in the local store
    /// (remote or not yet imported). Kept explicit, never guessed.
    pub unresolved: Vec<RecordId>,
    pub truncated: bool,
}

fn wanted(rel: &Relation, opts: &TraversalOptions) -> bool {
    (opts.include_retracted || !rel.retracted) && (opts.kinds.is_empty() || opts.kinds.contains(&rel.kind))
}

/// Outgoing edges of a record.
pub fn outgoing<S: Store>(store: &S, id: &RecordId, kinds: &[RelationKind]) -> Result<Vec<Edge>> {
    let Some(record) = store.get(id)? else { return Ok(Vec::new()) };
    Ok(record
        .active_relations()
        .filter(|r| kinds.is_empty() || kinds.contains(&r.kind))
        .map(|r| Edge { source: id.clone(), relation: r.clone() })
        .collect())
}

/// Incoming edges of a record, optionally only from sources of some kinds.
pub fn incoming<S: Store>(store: &S, id: &RecordId, kinds: &[RelationKind], from: &[RecordKind]) -> Result<Vec<Edge>> {
    Ok(store
        .incoming(id)?
        .into_iter()
        .filter(|e| !e.relation.retracted)
        .filter(|e| kinds.is_empty() || kinds.contains(&e.relation.kind))
        .filter(|e| from.is_empty() || from.contains(&e.source_kind))
        .map(|e| Edge { source: e.source, relation: e.relation })
        .collect())
}

/// Breadth-first traversal from `start`.
pub fn traverse<S: Store>(store: &S, start: &RecordId, opts: &TraversalOptions) -> Result<Traversal> {
    let mut out = Traversal::default();
    let mut seen: HashSet<RecordId> = HashSet::from([start.clone()]);
    let mut edge_seen = HashSet::new();
    let mut queue = VecDeque::from([(start.clone(), 0usize)]);
    while let Some((id, depth)) = queue.pop_front() {
        let Some(record) = store.get(&id)? else {
            out.unresolved.push(id);
            continue;
        };
        out.nodes.push((id.clone(), depth));
        if depth >= opts.max_depth {
            continue;
        }
        let mut next = Vec::new();
        if opts.direction != Direction::Incoming {
            for rel in record.relations.iter().filter(|r| wanted(r, opts)) {
                next.push((Edge { source: id.clone(), relation: rel.clone() }, rel.target.clone()));
            }
        }
        if opts.direction != Direction::Outgoing {
            for e in store.incoming(&id)? {
                if wanted(&e.relation, opts) {
                    let src = e.source.clone();
                    next.push((Edge { source: e.source, relation: e.relation }, src));
                }
            }
        }
        for (edge, neighbour) in next {
            if edge_seen.insert((edge.source.clone(), edge.relation.id)) {
                out.edges.push(edge);
            }
            if seen.contains(&neighbour) {
                continue;
            }
            if seen.len() >= opts.max_nodes {
                out.truncated = true;
                continue;
            }
            seen.insert(neighbour.clone());
            queue.push_back((neighbour, depth + 1));
        }
    }
    Ok(out)
}

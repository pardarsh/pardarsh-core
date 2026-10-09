//! Visibility and attribution disclosure.
//!
//! These are primitives, not a policy: the application decides which
//! audiences a viewer holds and who is privileged. Everything that leaves the
//! system for a viewer (API responses, exports) should pass through
//! [`view_record`] / [`view_history`] so that restricted records, events and
//! sources, and withheld identities, are never exposed by accident.

use std::collections::BTreeSet;

use crate::event::{Change, Event, EventKind};
use crate::model::{ActorRef, Attribution, Disclosure, Record, Visibility};

/// Who is looking.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Viewer {
    pub actor: Option<ActorRef>,
    pub audiences: BTreeSet<String>,
    /// Privileged viewers (e.g. moderators) see everything.
    pub privileged: bool,
}

impl Viewer {
    /// An anonymous member of the public.
    pub fn public() -> Self {
        Self::default()
    }

    /// Sees everything. Use for internal processing and full-fidelity export.
    pub fn privileged() -> Self {
        Self { actor: None, audiences: BTreeSet::new(), privileged: true }
    }

    pub fn actor(actor: ActorRef) -> Self {
        Self { actor: Some(actor), ..Self::default() }
    }

    pub fn with_audience(mut self, a: impl Into<String>) -> Self {
        self.audiences.insert(a.into());
        self
    }

    pub fn can_see(&self, v: &Visibility) -> bool {
        match v {
            Visibility::Public => true,
            Visibility::Restricted { audiences } => {
                self.privileged || audiences.iter().any(|a| self.audiences.contains(a))
            }
        }
    }

    fn mask(&self, a: &Attribution) -> Attribution {
        let own = self.actor.as_ref() == Some(&a.actor);
        if a.disclosure == Disclosure::Withheld && !self.privileged && !own {
            Attribution { actor: ActorRef::Anonymous, disclosure: Disclosure::Withheld }
        } else {
            a.clone()
        }
    }
}

/// The record as `viewer` may see it, or `None` if it is not visible.
pub fn view_record(record: &Record, viewer: &Viewer) -> Option<Record> {
    if !viewer.can_see(&record.visibility) {
        return None;
    }
    let mut r = record.clone();
    r.sources.retain(|s| viewer.can_see(&s.visibility));
    for s in r.sources.iter_mut() {
        if let Some(a) = &s.submitted_by {
            s.submitted_by = Some(viewer.mask(a));
        }
    }
    for a in r.actors.iter_mut() {
        a.attribution = viewer.mask(&a.attribution);
    }
    for rel in r.relations.iter_mut() {
        rel.asserted_by = viewer.mask(&rel.asserted_by);
        rel.sources.retain(|s| viewer.can_see(&s.visibility));
    }
    Some(r)
}

/// The events of a history that `viewer` may see, with restricted content
/// removed and withheld identities masked. Callers must first check that the
/// record's current state is visible (see [`view_record`]).
pub fn view_history(events: &[Event], viewer: &Viewer) -> Vec<Event> {
    let mut out = Vec::new();
    for e in events {
        if !viewer.can_see(&e.visibility) {
            continue;
        }
        let mut e = e.clone();
        e.attribution = viewer.mask(&e.attribution);
        match &mut e.kind {
            EventKind::Created { record } => {
                // Whether the record is visible at all is decided on its
                // *current* state by the caller; here only filter contents.
                let original = std::mem::take(&mut record.visibility);
                if let Some(mut r) = view_record(record, viewer) {
                    r.visibility = original;
                    **record = r;
                }
            }
            EventKind::Amended { changes } | EventKind::Corrected { changes, .. } => {
                changes.retain(|c| match c {
                    Change::AddSource(s) => viewer.can_see(&s.visibility),
                    _ => true,
                });
                for c in changes.iter_mut() {
                    if let Change::AddActor(a) = c {
                        a.attribution = viewer.mask(&a.attribution);
                    }
                }
            }
            EventKind::RelationAdded { relation } => {
                relation.asserted_by = viewer.mask(&relation.asserted_by);
                relation.sources.retain(|s| viewer.can_see(&s.visibility));
            }
            _ => {}
        }
        out.push(e);
    }
    out
}

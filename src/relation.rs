//! Typed relationships between records.
//!
//! A relation is stored on its *source* record as an outgoing edge
//! `source --kind--> target`. Every core relation kind has a documented
//! direction and meaning ([`RelationKind::meaning`]) and validation rules
//! ([`RelationKind::target_kinds`], [`RelationKind::requires_same_kind`]).
//! A relation also carries a [`Certainty`] so that a link never implies more
//! than its sources support.

use serde::{Deserialize, Serialize};

use crate::id::{RecordId, RelationId, open_vocabulary};
use crate::model::{Attribution, RecordKind, Timestamp};
use crate::provenance::SourceRef;

open_vocabulary!(
    /// The controlled relation vocabulary (plus namespaced extensions).
    RelationKind {
        Supports => "supports",
        Contradicts => "contradicts",
        RespondsTo => "responds_to",
        Duplicates => "duplicates",
        Supersedes => "supersedes",
        Corrects => "corrects",
        Retracts => "retracts",
        DerivedFrom => "derived_from",
        Cites => "cites",
        ProposesChangeTo => "proposes_change_to",
        DecidedBy => "decided_by",
        IssuedBy => "issued_by",
        AssignedTo => "assigned_to",
        OwnedBy => "owned_by",
        Implements => "implements",
        FundedBy => "funded_by",
        ProcuredThrough => "procured_through",
        Affects => "affects",
        LocatedAt => "located_at",
        Fulfills => "fulfills",
        MeasuredBy => "measured_by",
        ResultedIn => "resulted_in",
        Disputes => "disputes",
        RelatedTo => "related_to",
    }
);

impl RelationKind {
    /// Meaning of `source --kind--> target`.
    pub fn meaning(&self) -> &'static str {
        match self {
            Self::Supports => "source is offered as support for target",
            Self::Contradicts => "source is offered as contradicting target",
            Self::RespondsTo => "source is a response to target",
            Self::Duplicates => "source describes the same thing as target (target is canonical)",
            Self::Supersedes => "source replaces target",
            Self::Corrects => "source corrects an error in target",
            Self::Retracts => "source withdraws target",
            Self::DerivedFrom => "source was derived from target",
            Self::Cites => "source cites target",
            Self::ProposesChangeTo => "source proposes a change to target",
            Self::DecidedBy => "source was decided by decision target",
            Self::IssuedBy => "source was issued by actor target",
            Self::AssignedTo => "source is assigned to actor target",
            Self::OwnedBy => "source is owned by actor target",
            Self::Implements => "source implements target",
            Self::FundedBy => "source is funded by target",
            Self::ProcuredThrough => "source was procured through target",
            Self::Affects => "source affects target",
            Self::LocatedAt => "source is located at target",
            Self::Fulfills => "source fulfills obligation target",
            Self::MeasuredBy => "source is measured by target",
            Self::ResultedIn => "source resulted in target",
            Self::Disputes => "source disputes target",
            Self::RelatedTo => "source is related to target in an unspecified way",
            Self::Extension(_) => "extension relation; meaning defined by its namespace",
        }
    }

    /// Kinds the *target* must have, when the target is known locally.
    pub fn target_kinds(&self) -> Option<&'static [RecordKind]> {
        const ACTOR: &[RecordKind] = &[RecordKind::Actor];
        const DECISION: &[RecordKind] = &[RecordKind::Decision];
        const ACTION: &[RecordKind] = &[RecordKind::Action];
        const MEASURE: &[RecordKind] = &[RecordKind::Outcome, RecordKind::Evidence];
        match self {
            Self::IssuedBy | Self::AssignedTo | Self::OwnedBy => Some(ACTOR),
            Self::DecidedBy => Some(DECISION),
            Self::Fulfills => Some(ACTION),
            Self::MeasuredBy => Some(MEASURE),
            _ => None,
        }
    }

    /// Relations that only make sense between records of the same kind.
    pub fn requires_same_kind(&self) -> bool {
        matches!(self, Self::Duplicates | Self::Supersedes)
    }
}

/// How certain the link is, as asserted by whoever recorded it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    /// Suggested but not checked (e.g. "this might be the responsible body").
    #[default]
    Unverified,
    /// Asserted by the recording actor on their own authority.
    Asserted,
    /// Backed by at least one cited source. Requires `sources` to be non-empty.
    Sourced,
    /// The link itself is contested.
    Disputed,
}

/// A relationship as stored on the source record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    pub id: RelationId,
    pub kind: RelationKind,
    pub target: RecordId,
    /// Where a remote target can be retrieved, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_locator: Option<String>,
    pub certainty: Certainty,
    pub asserted_by: Attribution,
    pub asserted_at: Timestamp,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Retracted relations are kept for history but excluded from traversal.
    #[serde(default)]
    pub retracted: bool,
}

/// Input for asserting a relation. Assertion metadata comes from the command.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelationDraft {
    pub kind: RelationKind,
    pub target: RecordId,
    #[serde(default)]
    pub target_locator: Option<String>,
    #[serde(default)]
    pub certainty: Certainty,
    #[serde(default)]
    pub sources: Vec<SourceRef>,
    #[serde(default)]
    pub note: Option<String>,
}

impl RelationDraft {
    pub fn new(kind: RelationKind, target: RecordId) -> Self {
        Self { kind, target, target_locator: None, certainty: Certainty::Asserted, sources: Vec::new(), note: None }
    }

    pub fn certainty(mut self, c: Certainty) -> Self {
        self.certainty = c;
        self
    }

    pub fn source(mut self, s: SourceRef) -> Self {
        self.sources.push(s);
        self.certainty = Certainty::Sourced;
        self
    }

    pub fn note(mut self, n: impl Into<String>) -> Self {
        self.note = Some(n.into());
        self
    }

    pub(crate) fn into_relation(self, asserted_by: Attribution, asserted_at: Timestamp) -> Relation {
        Relation {
            id: RelationId::generate(),
            kind: self.kind,
            target: self.target,
            target_locator: self.target_locator,
            certainty: self.certainty,
            asserted_by,
            asserted_at,
            sources: self.sources,
            note: self.note,
            retracted: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_round_trips() {
        for term in RelationKind::CORE {
            let k: RelationKind = term.parse().unwrap();
            assert_eq!(&k.as_str(), term);
            assert!(!k.meaning().is_empty());
        }
        let ext: RelationKind = "procurement:awarded_to".parse().unwrap();
        assert!(ext.is_extension());
        assert!("not_a_relation".parse::<RelationKind>().is_err());
    }
}

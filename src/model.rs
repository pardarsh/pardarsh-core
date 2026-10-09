//! The common record envelope.
//!
//! A [`Record`] is the current-state *projection* of a record's event
//! history (see [`crate::event`] and [`crate::projection`]). Applications
//! never mutate a `Record` directly; they append events through the
//! [`crate::repository::Repository`].

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::extension::ExtValue;
use crate::id::{RecordId, open_vocabulary};
use crate::provenance::SourceRef;
use crate::relation::Relation;
use crate::schema::SchemaVersion;

pub type Timestamp = DateTime<Utc>;

open_vocabulary!(
    /// The type of a record. The core kinds are deliberately broader than any
    /// one application: issues are only one of them.
    RecordKind {
        /// A problem, question, service failure, unmet commitment, or public concern.
        Issue => "issue",
        /// A document, URL, photograph, dataset, observation or other relevant material.
        Evidence => "evidence",
        /// An attributed statement that may be supported, disputed or qualified.
        Claim => "claim",
        /// A suggested remedy or change.
        Proposal => "proposal",
        /// A question, argument, objection, response or review.
        Comment => "comment",
        /// A formal or attributable choice by an actor or institution.
        Decision => "decision",
        /// A committed or required next step (action or obligation).
        Action => "action",
        /// Implementation activity (project or work item).
        Project => "project",
        /// A result or reported state of the world, with evidence and time.
        Outcome => "outcome",
        /// A person, institution, department, community group, contractor or other participant.
        Actor => "actor",
        /// A published source artifact (law, notice, report, dataset...).
        Source => "source",
    }
);

open_vocabulary!(
    /// The role an actor plays with respect to a record.
    ActorRoleKind {
        Creator => "creator",
        Author => "author",
        Reporter => "reporter",
        Issuer => "issuer",
        Publisher => "publisher",
        Reviewer => "reviewer",
        Subject => "subject",
        Responsible => "responsible",
        Contributor => "contributor",
    }
);

/// Who did or said something.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActorRef {
    /// An actor described by an `actor` record (local or remote).
    Record { id: RecordId },
    /// No identity was given or it was not retained.
    Anonymous,
    /// An automated process, named for auditability (e.g. `importer`).
    System { name: String },
}

/// Whether the identity of an attributed actor may be shown to viewers who
/// are neither the actor nor privileged. Withholding is *not* anonymity:
/// the attribution is retained for accountability and moderation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disclosure {
    #[default]
    Public,
    Withheld,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Attribution {
    pub actor: ActorRef,
    #[serde(default)]
    pub disclosure: Disclosure,
}

impl Attribution {
    pub fn public(actor: ActorRef) -> Self {
        Self { actor, disclosure: Disclosure::Public }
    }

    pub fn withheld(actor: ActorRef) -> Self {
        Self { actor, disclosure: Disclosure::Withheld }
    }

    pub fn system(name: &str) -> Self {
        Self::public(ActorRef::System { name: name.to_string() })
    }

    pub fn anonymous() -> Self {
        Self::public(ActorRef::Anonymous)
    }
}

/// An actor with an explicit role on a record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorRole {
    pub role: ActorRoleKind,
    #[serde(flatten)]
    pub attribution: Attribution,
}

/// Who may see a record, an event or a source.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum Visibility {
    /// Anyone, including anonymous readers and exports.
    #[default]
    Public,
    /// Only viewers holding one of the audiences (and privileged viewers).
    /// An empty audience set means privileged viewers only.
    Restricted { audiences: BTreeSet<String> },
}

impl Visibility {
    pub fn restricted<I: IntoIterator<Item = S>, S: Into<String>>(audiences: I) -> Self {
        Self::Restricted { audiences: audiences.into_iter().map(Into::into).collect() }
    }

    /// Visible only to privileged viewers.
    pub fn private() -> Self {
        Self::Restricted { audiences: BTreeSet::new() }
    }

    pub fn is_public(&self) -> bool {
        matches!(self, Self::Public)
    }
}

/// Generic lifecycle of a record. Application workflow states (for example
/// an issue's triage status) do *not* belong here; they live in extensions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Lifecycle {
    #[default]
    Active,
    /// Withdrawn by its author/issuer. History is preserved.
    Retracted,
    /// Replaced by another record. History is preserved.
    Superseded { by: RecordId },
}

/// The distinct times a record can carry. These are never conflated:
/// `recorded_at` is when *this system* first recorded it (ingestion time),
/// the others are claims about the world and are optional.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordTimes {
    pub recorded_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_from: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_until: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<Timestamp>,
}

/// Namespaced, typed extension fields attached to a record. Validated against
/// an [`crate::extension::ExtensionSchema`] registered for the namespace.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtensionData {
    pub schema_version: SchemaVersion,
    pub fields: BTreeMap<String, ExtValue>,
}

/// The common record envelope (current state).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub id: RecordId,
    pub kind: RecordKind,
    pub schema_version: SchemaVersion,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub times: RecordTimes,
    #[serde(default)]
    pub actors: Vec<ActorRole>,
    #[serde(default)]
    pub sources: Vec<SourceRef>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default)]
    pub lifecycle: Lifecycle,
    #[serde(default)]
    pub extensions: BTreeMap<String, ExtensionData>,
    /// Fields whose values were redacted (see ADR 0006).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redactions: Vec<crate::event::FieldPath>,
    /// Sequence number of the last applied event (starts at 1).
    pub version: u64,
    /// Recorded time of the last applied event.
    pub updated_at: Timestamp,
}

impl Record {
    pub fn extension_field(&self, namespace: &str, field: &str) -> Option<&ExtValue> {
        self.extensions.get(namespace).and_then(|e| e.fields.get(field))
    }

    /// Active (non-retracted) relations.
    pub fn active_relations(&self) -> impl Iterator<Item = &Relation> {
        self.relations.iter().filter(|r| !r.retracted)
    }

    pub fn actors_with_role<'a>(&'a self, role: &'a ActorRoleKind) -> impl Iterator<Item = &'a Attribution> {
        self.actors.iter().filter(move |a| &a.role == role).map(|a| &a.attribution)
    }
}

/// Input for creating a record. The repository assigns the id, version and
/// recorded time; clients cannot supply them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RecordDraft {
    pub kind: RecordKind,
    pub title: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub published_at: Option<Timestamp>,
    #[serde(default)]
    pub effective_from: Option<Timestamp>,
    #[serde(default)]
    pub effective_until: Option<Timestamp>,
    #[serde(default)]
    pub observed_at: Option<Timestamp>,
    #[serde(default)]
    pub actors: Vec<ActorRole>,
    #[serde(default)]
    pub sources: Vec<SourceRef>,
    #[serde(default)]
    pub relations: Vec<crate::relation::RelationDraft>,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default)]
    pub extensions: BTreeMap<String, ExtensionData>,
}

impl RecordDraft {
    pub fn new(kind: RecordKind, title: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            summary: None,
            body: None,
            published_at: None,
            effective_from: None,
            effective_until: None,
            observed_at: None,
            actors: Vec::new(),
            sources: Vec::new(),
            relations: Vec::new(),
            visibility: Visibility::Public,
            extensions: BTreeMap::new(),
        }
    }

    pub fn summary(mut self, s: impl Into<String>) -> Self {
        self.summary = Some(s.into());
        self
    }

    pub fn body(mut self, s: impl Into<String>) -> Self {
        self.body = Some(s.into());
        self
    }

    pub fn actor(mut self, role: ActorRoleKind, attribution: Attribution) -> Self {
        self.actors.push(ActorRole { role, attribution });
        self
    }

    pub fn source(mut self, source: SourceRef) -> Self {
        self.sources.push(source);
        self
    }

    pub fn relation(mut self, relation: crate::relation::RelationDraft) -> Self {
        self.relations.push(relation);
        self
    }

    pub fn visibility(mut self, v: Visibility) -> Self {
        self.visibility = v;
        self
    }

    pub fn extension(mut self, namespace: &str, version: SchemaVersion, fields: BTreeMap<String, ExtValue>) -> Self {
        self.extensions.insert(namespace.to_string(), ExtensionData { schema_version: version, fields });
        self
    }
}

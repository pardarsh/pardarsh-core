//! # pardarsh-core
//!
//! Application-agnostic infrastructure for the Public Decision Graph:
//! stable identifiers, a common record envelope with typed extensions,
//! attributable append-only history, provenance, typed relationships,
//! validation, storage interfaces, visibility primitives, and a verifiable
//! interchange format.
//!
//! This crate knows nothing about any particular application. Issue
//! workflows, moderation policy, accounts and UI belong to applications
//! built on top of it.
//!
//! ```
//! use pardarsh_core::prelude::*;
//!
//! let origin = Origin::new("example.org").unwrap();
//! let mut repo = Repository::new(MemoryStore::new(origin), Validator::default());
//! let actor = Attribution::system("docs");
//! let issue = repo
//!     .create(RecordDraft::new(RecordKind::Issue, "Streetlight out on 4th Cross"), Meta::by(actor.clone()))
//!     .unwrap()
//!     .record;
//! let evidence = repo
//!     .create(
//!         RecordDraft::new(RecordKind::Evidence, "Photo after dark")
//!             .source(SourceRef::new("https://example.org/photo.jpg").authority(SourceAuthority::UserSubmitted))
//!             .relation(RelationDraft::new(RelationKind::Supports, issue.id.clone())),
//!         Meta::by(actor),
//!     )
//!     .unwrap()
//!     .record;
//! let supporting = pardarsh_core::graph::incoming(repo.store(), &issue.id, &[RelationKind::Supports], &[]).unwrap();
//! assert_eq!(supporting[0].source, evidence.id);
//! ```

pub mod access;
pub mod error;
pub mod event;
pub mod extension;
pub mod federation;
pub mod graph;
pub mod id;
pub mod interchange;
pub mod model;
pub mod projection;
pub mod provenance;
pub mod relation;
pub mod repository;
pub mod schema;
pub mod storage;
pub mod validation;

pub use error::{Error, Result};

/// The commonly used types.
pub mod prelude {
    pub use crate::access::{Viewer, view_history, view_record};
    pub use crate::error::{Error, Result};
    pub use crate::event::{Change, Event, EventKind, FieldPath};
    pub use crate::extension::{ExtValue, ExtensionRegistry, ExtensionSchema, FieldSpec, FieldType};
    pub use crate::id::{EventId, NamespacedName, Origin, RecordId, RelationId, SourceId};
    pub use crate::model::{
        ActorRef, ActorRole, ActorRoleKind, Attribution, Disclosure, Lifecycle, Record, RecordDraft, RecordKind,
        Timestamp, Visibility,
    };
    pub use crate::projection::TimelineEntry;
    pub use crate::provenance::{ContentHash, Derivation, SourceAuthority, SourceRef};
    pub use crate::relation::{Certainty, Relation, RelationDraft, RelationKind};
    pub use crate::repository::{Clock, Committed, ManualClock, Meta, Repository, SystemClock};
    pub use crate::schema::{ENVELOPE_SCHEMA_VERSION, SchemaVersion};
    #[cfg(feature = "sqlite")]
    pub use crate::storage::SqliteStore;
    pub use crate::storage::{MemoryStore, RecordQuery, Store};
    pub use crate::validation::{ValidationErrors, Validator};
}

use crate::id::{IdError, RecordId};
use crate::validation::ValidationErrors;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Id(#[from] IdError),
    #[error(transparent)]
    Validation(#[from] ValidationErrors),
    #[error("record not found: {0}")]
    NotFound(RecordId),
    #[error("version conflict on {id}: expected version {expected}, current version is {actual}")]
    Conflict { id: RecordId, expected: u64, actual: u64 },
    #[error("idempotency key {0:?} was already used for a different command")]
    IdempotencyConflict(String),
    #[error("operation not permitted: {0}")]
    NotPermitted(String),
    #[error("invalid history: {0}")]
    Projection(String),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("import rejected: {0}")]
    Import(String),
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Serialization(e.to_string())
    }
}

use super::ports::{BoxError, RepositoryError};
use crate::modules::items::domain::ValidationError;

/// Every way an items use case can fail. Transport maps each variant to an
/// HTTP status; nothing here knows about HTTP.
#[derive(Debug, thiserror::Error)]
pub enum ItemError {
    #[error(transparent)]
    Invalid(#[from] ValidationError),
    #[error("item not found")]
    NotFound,
    #[error("an item with this name already exists")]
    NameTaken,
    #[error("item storage is full")]
    CapacityExceeded,
    #[error("storage failure: {0}")]
    Storage(#[source] BoxError),
}

impl From<RepositoryError> for ItemError {
    fn from(err: RepositoryError) -> Self {
        match err {
            RepositoryError::DuplicateName => Self::NameTaken,
            RepositoryError::CapacityExceeded => Self::CapacityExceeded,
            RepositoryError::Unavailable(source) => Self::Storage(source),
        }
    }
}

use crate::app::ApiError;
use crate::modules::items::application::ItemError;

/// The single place items errors acquire HTTP meaning.
impl From<ItemError> for ApiError {
    fn from(err: ItemError) -> Self {
        match err {
            ItemError::Invalid(e) => Self::Unprocessable(e.to_string()),
            ItemError::NotFound => Self::NotFound,
            ItemError::NameTaken => Self::Conflict(err.to_string()),
            ItemError::CapacityExceeded => Self::InsufficientStorage,
            ItemError::Storage(source) => Self::Internal(source),
        }
    }
}

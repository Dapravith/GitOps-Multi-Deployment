/// A value broke a domain rule. Messages are fixed strings written by us, so
/// they are safe to return to a client: they never echo the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{field}: {message}")]
pub struct ValidationError {
    pub field: &'static str,
    pub message: &'static str,
}

impl ValidationError {
    #[must_use]
    pub const fn new(field: &'static str, message: &'static str) -> Self {
        Self { field, message }
    }
}

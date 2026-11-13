//! What the application needs from the outside world.

use std::error::Error as StdError;
use std::future::Future;
use std::pin::Pin;

use crate::modules::items::domain::{Item, ItemId, ValidationError};

pub type BoxError = Box<dyn StdError + Send + Sync + 'static>;

/// Boxed future so the trait stays object-safe (`Arc<dyn ItemRepository>`)
/// without pulling in `async-trait`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// Another item already has this name (case-insensitive).
    #[error("name already in use")]
    DuplicateName,
    #[error("storage capacity exceeded")]
    CapacityExceeded,
    /// The backing store failed. The source is logged, never shown to clients.
    #[error("storage unavailable: {0}")]
    Unavailable(#[source] BoxError),
}

pub const DEFAULT_PAGE_LIMIT: u32 = 20;
pub const MAX_PAGE_LIMIT: u32 = 100;

/// A validated `limit` / `offset` window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    limit: u32,
    offset: u32,
}

impl Page {
    /// # Errors
    /// Returns [`ValidationError`] if `limit` is outside 1..=100.
    pub fn new(limit: Option<u32>, offset: Option<u32>) -> Result<Self, ValidationError> {
        let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT);
        if !(1..=MAX_PAGE_LIMIT).contains(&limit) {
            return Err(ValidationError::new("limit", "must be between 1 and 100"));
        }
        Ok(Self {
            limit,
            offset: offset.unwrap_or(0),
        })
    }

    #[must_use]
    pub const fn limit(self) -> u32 {
        self.limit
    }

    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemPage {
    pub items: Vec<Item>,
    pub total: usize,
}

/// Storage port. Implementations must enforce name uniqueness atomically
/// (the way a database unique index would), so two concurrent creates with
/// the same name cannot both succeed.
pub trait ItemRepository: Send + Sync + 'static {
    /// Fails with `DuplicateName` or `CapacityExceeded`.
    fn insert(&self, item: Item) -> BoxFuture<'_, Result<(), RepositoryError>>;

    fn get(&self, id: ItemId) -> BoxFuture<'_, Result<Option<Item>, RepositoryError>>;

    /// Items in a stable order (by id), plus the total count.
    fn list(&self, page: Page) -> BoxFuture<'_, Result<ItemPage, RepositoryError>>;

    /// Replace an existing item. `Ok(false)` if no item has that id.
    fn update(&self, item: Item) -> BoxFuture<'_, Result<bool, RepositoryError>>;

    /// `Ok(false)` if no item has that id.
    fn delete(&self, id: ItemId) -> BoxFuture<'_, Result<bool, RepositoryError>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_defaults_and_bounds() {
        assert_eq!(
            Page::new(None, None).unwrap(),
            Page {
                limit: 20,
                offset: 0
            }
        );
        assert_eq!(Page::new(Some(100), Some(7)).unwrap().offset(), 7);
        assert_eq!(Page::new(Some(0), None).unwrap_err().field, "limit");
        assert_eq!(Page::new(Some(101), None).unwrap_err().field, "limit");
    }
}

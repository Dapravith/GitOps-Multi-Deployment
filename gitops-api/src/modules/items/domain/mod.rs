//! Pure domain: types that cannot hold an invalid value. No I/O, no async,
//! no HTTP. Everything here is unit-testable in isolation.

mod error;
mod item;

pub use error::ValidationError;
pub use item::{DESCRIPTION_MAX_CHARS, Description, Item, ItemId, ItemName, NAME_MAX_CHARS};

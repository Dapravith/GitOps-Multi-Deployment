//! Use cases. Orchestrates the domain and talks to storage only through the
//! [`ports::ItemRepository`] trait, so it runs unchanged against the
//! in-memory adapter, a future Postgres adapter, or a test fake.

mod error;
pub mod ports;
mod service;

pub use error::ItemError;
pub use service::{ItemInput, ItemService};

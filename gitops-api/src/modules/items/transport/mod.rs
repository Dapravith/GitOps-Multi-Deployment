//! HTTP adapter: DTOs, handlers, routes, and the `ItemError → ApiError` map.

mod dto;
mod error;
mod handlers;
mod routes;

pub use routes::routes;

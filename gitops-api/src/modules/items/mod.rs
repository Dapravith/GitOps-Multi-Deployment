//! `items`: a small CRUD resource, protected by API-key bearer auth.
//!
//! | Method + path        | Success | Notes                               |
//! |----------------------|---------|-------------------------------------|
//! | `POST /items`        | 201     | `Location: /items/{id}`             |
//! | `GET /items`         | 200     | `?limit=1..=100` (20), `?offset=`   |
//! | `GET /items/{id}`    | 200     |                                     |
//! | `PUT /items/{id}`    | 200     | full replace                        |
//! | `DELETE /items/{id}` | 204     |                                     |
//!
//! Layers: `domain` (rules) ← `application` (use cases + ports) ←
//! `transport` (HTTP) and `infrastructure` (adapters). Only `transport`
//! knows axum; only `infrastructure` knows how items are stored.

pub mod application;
pub mod domain;
pub mod infrastructure;
mod transport;

pub use transport::routes;

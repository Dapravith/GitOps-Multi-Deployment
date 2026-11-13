//! Process-level primitives (secrets, credentials) that carry no HTTP or
//! feature knowledge. HTTP enforcement lives in `app::middleware::authentication`.

pub mod api_key;

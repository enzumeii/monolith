//! BFF endpoint wrappers over the internal `services/api`, split by upstream
//! route group (`auth`, `admins`). Shared server-only plumbing is in `client`.
//! All wrappers accept and return the shared `domain::dto` contracts.

pub mod admins;
pub mod auth;

mod client;

pub use admins::*;
pub use auth::*;

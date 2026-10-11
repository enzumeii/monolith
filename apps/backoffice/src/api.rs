//! Server functions that proxy to the upstream `services/api` over HTTP.
//!
//! Split by concern: [`auth`] (login/logout/refresh) and [`admins`] (admin
//! management plus the self-service profile update). Shared server-only HTTP
//! and session helpers live in [`client`].
//!
//! Every action takes the client-held [`crate::session::Session`], transparently
//! refreshes it via `/v1/auth/refresh` if the access token has expired
//! (rotating the stored refresh token), and returns the possibly-rotated
//! session alongside the result so the client can update its copy.

pub mod admins;
pub mod auth;

#[cfg(feature = "server")]
mod client;

pub use admins::*;
pub use auth::*;

#![allow(clippy::unwrap_used)]

//! Router integration tests backed by real PostgreSQL from Docker Compose.
//! Requests use `Router::oneshot`; browser/TCP tests mirror the same endpoint groups.

#[path = "e2e/mod.rs"]
mod e2e;

#![allow(clippy::unwrap_used)]

//! End-to-end tests. Each test spins up a real Postgres via testcontainers,
//! runs migrations, builds the real axum router, and calls the actual HTTP
//! endpoint — asserting the JSON response. Split per endpoint group under
//! `e2e/`.

#[path = "e2e/mod.rs"]
mod e2e;

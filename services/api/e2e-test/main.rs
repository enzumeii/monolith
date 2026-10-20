#![allow(clippy::unwrap_used)]

//! TCP E2E tests against a locally running API and Docker PostgreSQL.

mod common;
mod healthz;
mod process;
mod v1;

#![allow(clippy::unwrap_used)]

//! TCP E2E tests against a locally running API and Docker PostgreSQL.

use api::{client, config};

#[path = "../src/tests/blob_fixture.rs"]
mod blob_fixture;
mod common;
mod cron;
mod healthz;
mod process;
mod v1;

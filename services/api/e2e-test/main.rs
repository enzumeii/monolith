#![allow(clippy::unwrap_used)]

//! TCP E2E tests against a locally running API and Docker PostgreSQL.

use api::{client, config};

mod common;
mod cron;
mod healthz;
mod process;
#[path = "../src/tests/s3_fixture.rs"]
mod s3_fixture;
mod v1;

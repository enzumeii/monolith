pub mod accounting;
pub mod admin;
pub mod auth;
pub mod report_storage;

pub mod inventories;
pub mod orders;

mod pagination;

#[cfg(test)]
#[path = "tests/services/common.rs"]
pub(super) mod common;

pub(crate) mod cron;
pub(crate) mod email;

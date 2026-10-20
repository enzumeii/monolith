pub mod accounting;
pub mod admin;
pub mod auth;
pub mod cron_schedule;

pub mod inventories;
pub mod orders;

mod pagination;

#[cfg(test)]
#[path = "tests/services/common.rs"]
pub(super) mod common;

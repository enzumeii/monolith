pub mod accounting;
pub mod admin;
pub mod auth;
pub(crate) mod cron;

pub mod inventories;
pub mod orders;

mod pagination;

pub mod catalogue;

pub mod conventions;

#[cfg(test)]
#[path = "tests/services/common.rs"]
pub(super) mod common;

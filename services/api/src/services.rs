pub mod accounting;
pub mod admin;
pub mod auth;
pub mod checkout_completion;
pub(crate) mod cron;

pub mod inventories;
pub mod orders;

mod pagination;

pub mod catalogue;

pub mod conventions;
pub mod files;

#[cfg(test)]
#[path = "tests/services/common.rs"]
pub(super) mod common;

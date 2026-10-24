pub mod accounting;
pub mod admin;
pub mod auth;
pub(crate) mod checkout;
pub mod checkout_completion;
pub(crate) mod cron;

pub mod inventories;
pub mod orders;
pub mod payments;

mod pagination;

pub mod catalogue;

pub mod conventions;
pub mod files;

#[cfg(test)]
#[path = "tests/services/common.rs"]
pub(super) mod common;

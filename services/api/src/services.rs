pub mod accounting;
pub mod admin;
pub mod auth;

pub mod inventories;
pub mod orders;
pub mod stripe_payments;

mod pagination;

#[cfg(test)]
#[path = "tests/services/common.rs"]
pub(super) mod common;

pub(crate) mod cron;
pub(crate) mod paypal_payments;

pub(crate) mod checkout_completion;

pub(crate) mod checkout_verification;

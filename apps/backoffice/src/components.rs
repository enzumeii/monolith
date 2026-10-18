use dioxus::prelude::*;

mod confirm;
mod form;

pub use confirm::Confirm;
pub use form::{ErrorMessage, FormField};

pub const SITE_LOGO: Asset = asset!("/assets/logo.png");

use dioxus::prelude::*;

mod confirm;
mod form;
mod loading;
mod nav_layout;

pub use confirm::Confirm;
pub use form::{ErrorMessage, FormField};
pub(crate) use loading::{LoadingOverlay, LoadingState};
pub(crate) use nav_layout::NavLayout;

pub const SITE_LOGO: Asset = asset!("/assets/logo.png");

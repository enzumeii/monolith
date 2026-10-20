use dioxus::prelude::*;

mod confirm;
mod form;
mod loading;
mod nav_layout;
mod report_table;

pub use confirm::Confirm;
pub use form::{ErrorMessage, FormField};
pub(crate) use loading::{LoadingOverlay, LoadingState};
pub(crate) use nav_layout::NavLayout;

pub(crate) use report_table::{DataTable, DownloadFile, DownloadTable, TableData};

pub const SITE_LOGO: Asset = asset!("/assets/logo.png");

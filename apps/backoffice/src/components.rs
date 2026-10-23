use dioxus::prelude::*;

mod confirm;
mod date_time;
pub(crate) mod draft_navigation;
mod form;
mod heading;
mod loading;
mod nav_layout;
mod popup;
mod report_skeleton;
mod report_table;
pub(crate) mod request_state;
pub(crate) use report_skeleton::ReportSkeleton;

pub use confirm::Confirm;
pub use date_time::{DateTimePicker, TimePicker};
pub(crate) use draft_navigation::DraftDepartureGuard;
pub use form::{ErrorMessage, FormField};
pub(crate) use heading::{Heading, HeadingLevel};
pub(crate) use loading::{LoadingOverlay, LoadingState};
pub(crate) use nav_layout::NavLayout;
pub(crate) use popup::Popup;

pub(crate) use report_table::{DataTable, DownloadFile, DownloadTable, TableData};

pub const SITE_LOGO: Asset = asset!("/assets/logo.png");

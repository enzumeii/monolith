//! The app's pages, one module per page plus the shared navbar.

mod admin;
mod error;
mod home;
mod login;
mod navbar;
mod profile;

pub use admin::Admins;
pub use error::NotFound;
pub use home::Home;
pub use login::Login;
pub use navbar::Navbar;
pub use profile::Profile;

mod reports;
pub use reports::Reports;

mod triggers;
pub use triggers::Triggers;

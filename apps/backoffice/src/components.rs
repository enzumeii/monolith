//! UI components, one file per page, plus shared chrome (`navbar`) and a
//! fallback error page (`error`).

mod admins;
mod error;
mod home;
mod login;
mod navbar;
mod profile;

pub use admins::Admins;
pub use error::NotFound;
pub use home::Home;
pub use login::Login;
pub use navbar::Navbar;
pub use profile::Profile;

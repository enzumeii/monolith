//! UI components, one module per page plus shared chrome (navbar), form
//! primitives (form), and the error fallback (error). The admin management
//! page lives in the `admin` submodule.

mod admin;
mod error;
mod form;
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

//! Top navigation bar: brand, two tabs (Admin List for super admins, Profile),
//! and a logout button on the right.

use dioxus::prelude::*;
use domain::AdminRole;

use crate::route::Route;
use crate::session::{self, SESSION};

#[component]
pub fn Navbar() -> Element {
    let session = SESSION.read().clone();
    let is_super = matches!(
        session.as_ref().map(|s| s.role),
        Some(AdminRole::SuperAdmin)
    );

    rsx! {
        nav { class: "topbar",
            div { class: "brand",
                Link { to: Route::Home {}, "Backoffice" }
            }
            div { class: "tabs",
                if is_super {
                    Link { class: "tab", to: Route::Admins {}, "Admin List" }
                }
                Link { class: "tab", to: Route::Profile {}, "Profile" }
            }
            div { class: "topbar-actions",
                if let Some(s) = &session {
                    span { class: "whoami", "{s.username}" }
                }
                button {
                    class: "logout",
                    onclick: move |_| session::sign_out(),
                    "Logout"
                }
            }
        }
    }
}

//! Top navigation bar: brand, two tabs (Admin List for super admins, Profile),
//! and a logout button on the right.

use dioxus::prelude::*;
use domain::AdminRole;

use crate::route::Route;
use crate::session::{self, SESSION};

#[component]
pub fn Navbar() -> Element {
    let session = SESSION.read().clone();
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
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
                if let Some(message)=error() {span {role:"alert", "{message}"}}
                button {
                    class: "logout",
                    disabled: busy(),
                    onclick: move |_| {
                        busy.set(true);error.set(None);
                        spawn(async move { if let Err(e)=session::sign_out().await {error.set(Some(e.to_string()));busy.set(false);} });
                    },
                    "Logout"
                }
            }
        }
    }
}

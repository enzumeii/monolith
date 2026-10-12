//! Fallback error page.

use dioxus::prelude::*;

use crate::route::Route;

/// Rendered for any URL that doesn't match a known route (`/:..segments`).
///
/// Serves as the "something went wrong" surface: an unrecognized page the
/// router falls back to, rendered inside the navbar layout so the user can
/// navigate back.
#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    let path = segments.join("/");
    rsx! {
        main { class: "page",
            div { class: "panel error-page",
                h2 { "Page not found" }
                p { class: "meta", "No page matches \"/{path}\"." }
                Link { class: "tab", to: Route::Home {}, "Back to home" }
            }
        }
    }
}

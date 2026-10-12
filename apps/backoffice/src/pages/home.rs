//! Home route (`/`). Intentionally empty — the navbar provides navigation.

use dioxus::prelude::*;

#[component]
pub fn Home() -> Element {
    rsx! {
        main { class: "page home-empty" }
    }
}

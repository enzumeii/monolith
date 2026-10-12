//! Generic confirmation dialog for destructive/important actions.

use dioxus::prelude::*;

#[component]
pub fn Confirm(
    open: bool,
    title: String,
    message: String,
    confirm_label: String,
    danger: bool,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    if !open {
        return rsx! {};
    }
    rsx! {
        div { class: "modal-overlay",
            div { class: "modal", role: "dialog", aria_modal: "true",
                h3 { class: "modal-title", "{title}" }
                p { class: "modal-message", "{message}" }
                div { class: "modal-actions",
                    button {
                        class: "ghost",
                        onclick: move |_| on_cancel(()),
                        "Cancel"
                    }
                    button {
                        class: if danger { "danger" } else { "primary" },
                        autofocus: true,
                        onclick: move |_| on_confirm(()),
                        "{confirm_label}"
                    }
                }
            }
        }
    }
}

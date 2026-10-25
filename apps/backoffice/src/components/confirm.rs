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
    let (id, mounted) = ui::use_native_dialog(open, false);

    rsx! {
        dialog {
            id: "{id}",
            class: "modal",
            aria_labelledby: "{id}-title",
            aria_describedby: "{id}-message",
            onmounted: move |_| mounted(()),
            oncancel: move |event| {
                event.prevent_default();
                on_cancel(());
            },
            h3 { id: "{id}-title", class: "modal-title", "{title}" }
            p { id: "{id}-message", class: "modal-message", "{message}" }
            div { class: "modal-actions",
                button {
                    r#type: "button",
                    class: "ghost",
                    autofocus: true,
                    onclick: move |_| on_cancel(()),
                    "Cancel"
                }
                button {
                    r#type: "button",
                    class: if danger { "danger" } else { "primary" },
                    onclick: move |_| on_confirm(()),
                    "{confirm_label}"
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/components/confirm.rs"]
mod tests;

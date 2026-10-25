use dioxus::prelude::*;
use ui::use_native_dialog;

#[component]
pub fn Popup(
    open: bool,
    title: String,
    children: Element,
    #[props(default)] class: String,
    on_close: EventHandler<()>,
    #[props(default)] close_disabled: bool,
) -> Element {
    let (id, mounted) = use_native_dialog(open, true);
    rsx! {
        dialog { id: "{id}", class: "modal popup {class}", aria_labelledby: "{id}-title",
            onmounted: move |_| mounted(()),
            oncancel: move |event| { event.prevent_default(); if !close_disabled { on_close(()); } },
            header { class: "popup-header",
                h3 { id: "{id}-title", class: "modal-title", "{title}" }
                button { r#type: "button", class: "ghost popup-close", disabled: close_disabled,
                    aria_label: "Close {title}", onclick: move |_| on_close(()), "×" }
            }
            {children}
        }
    }
}

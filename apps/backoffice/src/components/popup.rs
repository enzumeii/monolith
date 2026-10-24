use dioxus::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIALOG: AtomicUsize = AtomicUsize::new(1);

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

pub(super) fn use_native_dialog(open: bool, dismiss_on_backdrop: bool) -> (String, Callback<()>) {
    let id = use_hook(|| format!("dialog-{}", NEXT_DIALOG.fetch_add(1, Ordering::Relaxed)));
    let mut dialog = use_signal(|| None::<document::Eval>);
    use_effect(use_reactive!(|open| {
        if let Some(dialog) = *dialog.peek() {
            let _ = dialog.send(Some(open));
        }
    }));
    use_drop(move || {
        if let Some(dialog) = *dialog.peek() {
            let _ = dialog.send(None::<bool>);
        }
    });
    let mounted_id = id.clone();
    let mounted = use_callback(move |()| {
        let bridge = document::eval(include_str!("popup.js"));
        let _ = bridge.send((&mounted_id, Some(open), dismiss_on_backdrop));
        dialog.set(Some(bridge));
    });
    (id, mounted)
}

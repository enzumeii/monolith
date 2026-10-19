use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub(crate) struct LoadingState(pub Signal<bool>);

#[component]
pub fn LoadingOverlay() -> Element {
    use_effect(|| {
        document::eval("document.getElementById('page-loading')?.focus();");
    });
    rsx! {
        div { class: "modal-overlay loading-overlay", id: "page-loading", role: "dialog", aria_modal: "true", aria_label: "Loading", tabindex: "-1",
            onkeydown: move |event| event.prevent_default(),
            onwheel: move |event| event.prevent_default(),
            div { class: "modal", p { role: "status", "Working… Please wait." } }
        }
    }
}

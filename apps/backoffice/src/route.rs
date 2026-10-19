use dioxus::prelude::*;

use crate::pages::{Admins, Home, Navbar, NotFound, Profile, Reports, Triggers};

#[derive(Clone, Debug, PartialEq, Routable)]
pub enum Route {
    #[layout(NavLayout)]
    #[route("/")]
    Home {},
    #[route("/admins")]
    Admins {},
    #[route("/reports")]
    Reports {},
    #[route("/triggers")]
    Triggers {},
    #[route("/profile")]
    Profile {},
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

#[derive(Clone, Copy)]
pub(crate) struct TriggerPending(pub Signal<bool>);

#[component]
fn NavLayout() -> Element {
    let pending = use_signal(|| false);
    use_context_provider(|| TriggerPending(pending));
    use_effect(move || {
        if pending() {
            document::eval("document.getElementById('trigger-loading')?.focus();");
        }
    });
    rsx! {
        div { "inert": pending().then_some(""),
            Navbar {}
            Outlet::<Route> {}
        }
        if pending() {
            div { class: "modal-overlay trigger-loading", id: "trigger-loading", role: "dialog", aria_modal: "true", aria_label: "Running trigger", tabindex: "-1",
                onkeydown: move |event| event.prevent_default(),
                onwheel: move |event| event.prevent_default(),
                div { class: "modal", p { role: "status", "Running trigger… Please wait." } }
            }
        }
    }
}

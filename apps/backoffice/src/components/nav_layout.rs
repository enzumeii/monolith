use dioxus::prelude::*;

use super::{LoadingOverlay, LoadingState};
use crate::{pages::Navbar, route::Route};

#[component]
pub fn NavLayout() -> Element {
    let pending = use_signal(|| false);
    use_context_provider(|| LoadingState(pending));
    rsx! {
        div { "inert": pending().then_some(""),
            Navbar {}
            Outlet::<Route> {}
        }
        if pending() { LoadingOverlay {} }
    }
}

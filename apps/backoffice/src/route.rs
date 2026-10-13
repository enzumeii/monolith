use dioxus::prelude::*;

use crate::pages::{Admins, Home, Navbar, NotFound, Profile};

#[derive(Clone, Debug, PartialEq, Routable)]
pub enum Route {
    #[layout(NavLayout)]
    #[route("/")]
    Home {},
    #[route("/admins")]
    Admins {},
    #[route("/profile")]
    Profile {},
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

#[component]
fn NavLayout() -> Element {
    rsx! {
        Navbar {}
        Outlet::<Route> {}
    }
}

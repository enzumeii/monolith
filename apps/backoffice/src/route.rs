//! Route table and the authenticated layout.
//!
//! The navbar is part of the layout so it renders inside the router context
//! and its `Link`s / `use_navigator` calls work. Every authenticated page is a
//! child of the layout and renders into the `Outlet`. Unrecognized routes fall
//! back to the catch-all `NotFound` page.

use dioxus::prelude::*;

use crate::components::{Admins, Home, Navbar, NotFound, Profile};

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

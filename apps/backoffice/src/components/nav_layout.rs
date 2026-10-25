use crate::{pages::Navbar, route::Route};
use dioxus::prelude::*;
#[component]
pub fn NavLayout() -> Element {
    rsx! { Navbar {} Outlet::<Route> {} }
}

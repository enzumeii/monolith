mod catalogue;
mod delete;
mod detail;
mod list;
mod logo;
mod metadata;
mod navigation;
mod state;

pub use list::Conventions;

use crate::{
    components::{Heading, request_state::use_owner},
    route::Route,
};
use dioxus::prelude::*;

#[component]
pub fn ConventionCreate() -> Element {
    let owner = use_owner();
    rsx! { document::Stylesheet { href: asset!("/assets/conventions.css") } main { class: "page conventions-page", Heading { title: "Create convention", back: rsx! { Link { to: Route::Conventions { query: String::new() }, class: "button-link ghost", aria_label: "Back to conventions", title: "Back to conventions", "<" } } } if let Some(owner) = owner() { metadata::CreateEditor { key: "{owner:?}" } } } }
}
#[component]
pub fn ConventionDetail(id: String) -> Element {
    let owner = use_owner();
    rsx! { document::Stylesheet { href: asset!("/assets/conventions.css") } main { class: "page conventions-page", if let Some(owner) = owner() { detail::Detail { key: "{owner:?}:{id}", id } } } }
}
#[cfg(test)]
#[path = "../tests/pages/conventions.rs"]
mod tests;

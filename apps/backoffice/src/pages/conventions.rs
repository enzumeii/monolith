mod catalogue;
mod detail;
mod list;
mod logo;
mod metadata;
mod state;

pub use list::Conventions;

use crate::{components::request_state::use_owner, route::Route};
use dioxus::prelude::*;

#[component]
pub fn ConventionCreate() -> Element {
    let owner = use_owner();
    rsx! { document::Stylesheet { href: asset!("/assets/conventions.css") } main { class: "page conventions-page", h2 { class: "convention-create-heading", Link { to: Route::Conventions { query: String::new() }, class: "button-link ghost convention-back", aria_label: "Back to conventions", title: "Back to conventions", "<" } span { "Create convention" } } if let Some(owner) = owner() { metadata::CreateEditor { key: "{owner:?}" } } } }
}
#[component]
pub fn ConventionDetail(id: String) -> Element {
    let owner = use_owner();
    rsx! { document::Stylesheet { href: asset!("/assets/conventions.css") } main { class: "page conventions-page", h2 { "Convention administration" } if let Some(owner) = owner() { detail::Detail { key: "{owner:?}:{id}", id } } } }
}
#[cfg(test)]
#[path = "../tests/pages/conventions.rs"]
mod tests;

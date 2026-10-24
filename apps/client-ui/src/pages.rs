pub(crate) mod catalogue;
mod convention;
mod directory;
mod filters;
mod navbar;
mod product;

pub use catalogue::CataloguePage;
pub use convention::CartPage;
pub(crate) use convention::{ConventionContent, ConventionFrame, ConventionView};
pub use directory::Directory;
use navbar::PortalNavbar;
pub use product::ProductPage;

use crate::route::Route;
use dioxus::prelude::*;
use domain::dto::PublicConvention;
use ui::{Heading, MarkdownPreview};

#[component]
pub fn PortalLayout() -> Element {
    rsx! { Outlet::<Route> {} }
}

#[component]
pub fn NotFound(segments: Vec<String>) -> Element {
    rsx! {
        PortalNavbar {}
        main { class: "page", Heading { title: "Page unavailable" } p { "This page could not be found." } Link { to: Route::Directory {}, "Browse conventions" } }
    }
}

pub(crate) fn safe_image(url: &str) -> bool {
    reqwest::Url::parse(url)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}

#[component]
fn ConventionHeader(convention: PublicConvention) -> Element {
    rsx! {
        Heading { title: convention.name.clone() }
        div { class: "portal-event",
            if let Some(url) = convention.logo_preview_url.as_ref().filter(|url| safe_image(url)) {
                img { class: "portal-logo", src: "{url}", alt: "{convention.name} logo" }
            } else { div { class: "portal-logo portal-placeholder", aria_hidden: "true", "✦" } }
            div { p { class: "meta", "{convention.country_code} · {convention.event_starts_on} – {convention.event_ends_on}" } MarkdownPreview { markdown: convention.description_markdown } }
        }
    }
}

#[cfg(test)]
#[path = "tests/pages.rs"]
pub(crate) mod tests;

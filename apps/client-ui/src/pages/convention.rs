use super::ConventionHeader;
use crate::{api, cart::CartProvider};
use dioxus::prelude::*;
use ui::{ErrorMessage, LoadingMessage};

#[component]
pub fn CartPage(slug: String, checkout_id: String) -> Element {
    rsx! { ConventionPage { key: "{slug}", slug, checkout_id: Some(checkout_id) } }
}

#[component]
pub fn ConventionPage(slug: String, checkout_id: Option<String>) -> Element {
    let mut catalogue = use_resource(move || {
        let slug = slug.clone();
        async move { api::conventions::catalogue(&slug).await }
    });
    let state = catalogue.read().clone();
    rsx! { main { class: "page portal-page",
        match state {
            None => rsx! { LoadingMessage { message: "Loading convention…" } },
            Some(Err(error)) => rsx! {
                ErrorMessage { message: Some(if error.status == 404 { "This convention is not available for preorders.".into() } else { error.message }) }
                button { onclick: move |_| catalogue.restart(), "Retry convention" }
                Link { to: crate::route::Route::Directory {}, "All conventions" }
            },
            Some(Ok(catalogue)) => rsx! {
                div { key: "{catalogue.convention.id}",
                    ConventionHeader { convention: catalogue.convention.clone() }
                    CartProvider { catalogue, checkout_id }
                }
            },
        }
    } }
}

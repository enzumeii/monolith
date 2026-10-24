use super::{ConventionHeader, navbar::PortalNavbar};
use crate::{api, cart::browser, route::Route};
use dioxus::prelude::*;
use ui::{ErrorMessage, Heading, LoadingMessage};

#[component]
pub fn Directory() -> Element {
    let mut conventions = use_resource(|| async {
        let (today, country) = browser::visitor().await.map_err(|message| api::ApiError {
            status: 503,
            message,
        })?;
        api::conventions::directory(today, country).await
    });
    let state = conventions.read().clone();
    rsx! { PortalNavbar {} main { class: "page portal-page",
        Heading { title: "Upcoming conventions" }
        p { class: "meta", "Choose a convention to browse its preorder collection." }
        match state {
            None => rsx! { LoadingMessage { message: "Loading conventions…" } },
            Some(Err(error)) => rsx! { ErrorMessage { message: Some(error.message) } button { onclick: move |_| conventions.restart(), "Retry conventions" } },
            Some(Ok(items)) => rsx! {
                if items.is_empty() { p { "There are no upcoming conventions with preorders available." } }
                div { class: "portal-directory",
                    for convention in items {
                        article { class: "panel", key: "{convention.id}",
                            ConventionHeader { convention: convention.clone() }
                            Link { class: "button-link primary", to: Route::CataloguePage { slug: convention.slug }, "Browse products" }
                        }
                    }
                }
            },
        }
    } }
}

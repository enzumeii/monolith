mod catalogue;
mod detail;
mod metadata;
mod state;

use crate::{
    api,
    components::{ErrorMessage, request_state::use_owner},
    route::Route,
    session,
};
use dioxus::prelude::*;
use domain::dto::ListConventionsQuery;

#[component]
pub fn Conventions(query: String) -> Element {
    let owner = use_owner();
    let mut retry = use_signal(|| 0_u64);
    let (page, enabled) = list_query(&query);
    let response = use_resource(use_reactive((&query,), move |(query,)| {
        let _ = owner();
        let _ = retry();
        let (page, preorder_enabled) = list_query(&query);
        async move {
            session::with_session(|| {
                api::conventions::list(ListConventionsQuery {
                    page: Some(page),
                    per_page: Some(25),
                    preorder_enabled,
                })
            })
            .await
        }
    }));
    let navigator = use_navigator();
    rsx! {
        document::Stylesheet { href: asset!("/assets/conventions.css") }
        main { class: "page conventions-page",
            h2 { "Conventions" }
            p { "Enabled conventions with eligible selections are intended for the customer homepage. Event dates describe the event; they do not schedule preorders." }
            div { class: "convention-actions",
                Link { to: Route::ConventionCreate {}, class: "primary", "Create convention" }
                label { r#for: "convention-filter", "Preorders" }
                select { id: "convention-filter", value: enabled.map_or("all", |value| if value { "true" } else { "false" }),
                    onchange: move |event| { navigator.push(Route::Conventions { query: match event.value().as_str() { "true" => "page=1&preorder_enabled=true".into(), "false" => "page=1&preorder_enabled=false".into(), _ => "page=1".into() } }); },
                    option { value: "all", "All conventions" } option { value: "false", "Disabled" } option { value: "true", "Enabled" }
                }
            }
            if response.pending() || response.read().is_none() { p { role: "status", "Loading conventions…" } }
            else if let Some(result) = response.read().as_ref() {
                match result {
                    Err(error) => rsx! { ErrorMessage { message: Some(error.to_string()) } button { onclick: move |_| retry += 1, "Retry loading" } },
                    Ok(value) => rsx! {
                        if value.items.is_empty() { p { "No conventions match this filter." } }
                        div { class: "convention-table-scroll",
                            table { class: "convention-table",
                                caption { "Saved conventions" }
                                thead { tr { th { "Name" } th { "Event dates" } th { "Currency" } th { "Preorders" } } }
                                tbody { for item in &value.items { tr { key: "{item.id}",
                                    td { Link { to: Route::ConventionDetail { id: item.id.clone() }, "{item.name}" } }
                                    td { "{item.event_starts_on} – {item.event_ends_on}" } td { "{item.currency_code}" }
                                    td { if item.preorder_enabled { "Enabled" } else { "Disabled" } }
                                } } }
                            }
                        }
                        nav { class: "convention-actions", aria_label: "Convention pages",
                            if page > 1 { Link { to: Route::Conventions { query: page_query(page - 1, enabled) }, "Previous page" } }
                            span { "Page {value.page}" }
                            if value.has_next { Link { to: Route::Conventions { query: page_query(page + 1, enabled) }, "Next page" } }
                        }
                    },
                }
            }
        }
    }
}
#[component]
pub fn ConventionCreate() -> Element {
    let owner = use_owner();
    rsx! { document::Stylesheet { href: asset!("/assets/conventions.css") } main { class: "page conventions-page", h2 { "Create convention" } if let Some(owner) = owner() { metadata::CreateEditor { key: "{owner:?}" } } } }
}
#[component]
pub fn ConventionDetail(id: String) -> Element {
    let owner = use_owner();
    rsx! { document::Stylesheet { href: asset!("/assets/conventions.css") } main { class: "page conventions-page", h2 { "Convention administration" } if let Some(owner) = owner() { detail::Detail { key: "{owner:?}:{id}", id } } } }
}
fn list_query(query: &str) -> (u32, Option<bool>) {
    let mut url = reqwest::Url::parse("https://backoffice.invalid/").expect("static URL");
    url.set_query(Some(query.trim_start_matches('?')));
    let mut page = 1;
    let mut enabled = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "page" => page = value.parse().ok().filter(|v| *v > 0).unwrap_or(1),
            "preorder_enabled" => enabled = value.parse().ok(),
            _ => {}
        }
    }
    (page, enabled)
}
fn page_query(page: u32, enabled: Option<bool>) -> String {
    enabled.map_or_else(
        || format!("page={page}"),
        |value| format!("page={page}&preorder_enabled={value}"),
    )
}

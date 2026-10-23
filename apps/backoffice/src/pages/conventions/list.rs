use super::logo;
use crate::{
    api,
    components::{
        Confirm, ErrorMessage,
        request_state::{RequestState, use_owner},
    },
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
    let mut deleting = use_signal(|| None::<(String, String)>);
    let mut mutation = use_signal(RequestState::default);
    let mut delete_error = use_signal(|| None::<String>);
    use_effect(use_reactive((&query,), move |(_,)| {
        mutation.write().edited();
        deleting.set(None);
        delete_error.set(None);
    }));
    use_effect(move || {
        let _ = owner();
        mutation.write().edited();
        deleting.set(None);
        delete_error.set(None);
    });
    rsx! {
        document::Stylesheet { href: asset!("/assets/conventions.css") }
        main { class: "page conventions-page",
            header { class: "convention-list-heading",
                h2 { "Conventions" }
                Link { to: Route::ConventionCreate {}, class: "button-link primary", "Create convention" }
            }
            ErrorMessage { message: delete_error() }
            if response.read().is_none() || response.pending() { p { role: "status", "Loading conventions…" } }
            else if let Some(result) = response.read().as_ref() {
                match result {
                    Err(error) => rsx! { ErrorMessage { message: Some(error.to_string()) } button { class: "ghost", onclick: move |_| retry += 1, "Retry loading" } },
                    Ok(value) => rsx! {
                        if value.items.is_empty() { p { "No conventions match this filter." } }
                        div { class: "convention-table-scroll",
                            table { class: "convention-table convention-list-table",
                                caption { class: "visually-hidden", "Saved conventions" }
                                thead { tr { th { "Logo" } th { "Name" } th { "Event dates" } th { "Currency" } th { "Preorders" } th { "Actions" } } }
                                tbody { for item in &value.items { tr { key: "{item.id}",
                                    td { logo::LogoThumbnail { key: "{item.id}:{item.logo_preview_url:?}", name: item.name.clone(), preview_url: item.logo_preview_url.clone(), expires_at: item.logo_preview_url_expires_at.clone() } }
                                    td { class: "convention-value", "{item.name}" }
                                    td { class: "convention-value", "{item.event_starts_on} – {item.event_ends_on}" } td { class: "convention-value", "{item.currency_code}" }
                                    td { span { class: if item.preorder_enabled { "convention-status is-enabled" } else { "convention-status is-disabled" }, if item.preorder_enabled { "Enabled" } else { "Disabled" } } }
                                    td { div { class: "convention-row-actions",
                                        Link { to: Route::ConventionDetail { id: item.id.clone() }, class: "button-link ghost", aria_label: "Edit {item.name}", "Edit" }
                                        button { id: "delete-convention-{item.id}", r#type: "button", class: "danger", aria_label: "Delete {item.name}", disabled: mutation.read().pending() || owner().is_none(), onclick: { let id = item.id.clone(); let name = item.name.clone(); move |_| { delete_error.set(None); deleting.set(Some((id.clone(), name.clone()))); } }, "Delete" }
                                    } }
                                } } }
                            }
                        }
                        nav { class: "convention-actions", aria_label: "Convention pages",
                            if page > 1 { Link { to: Route::Conventions { query: page_query(page - 1, enabled) }, class: "button-link ghost", "Previous page" } }
                            span { "Page {value.page}" }
                            if value.has_next { Link { to: Route::Conventions { query: page_query(page + 1, enabled) }, class: "button-link ghost", "Next page" } }
                        }
                    },
                }
            }
        }
        Confirm {
            open: deleting().is_some(),
            title: deleting().map(|(_, name)| format!("Delete ‘{name}’?")).unwrap_or_default(),
            message: "This permanently deletes the convention and its selected variants. Uploaded logo files remain in storage.",
            confirm_label: "Delete convention", danger: true,
            on_cancel: move |()| deleting.set(None),
            on_confirm: move |()| {
                let Some((id, _)) = deleting() else { return; };
                let Some(ticket) = mutation.write().begin() else { return; };
                deleting.set(None); delete_error.set(None);
                let request_owner = owner();
                let previous_page = page > 1 && response.read().as_ref().and_then(|result| result.as_ref().ok()).is_some_and(|value| value.items.len() == 1);
                spawn(async move {
                    let result = session::with_session(|| api::conventions::delete(&id)).await;
                    if owner() != request_owner || !mutation.write().finish(ticket) { return; }
                    match result {
                        Ok(()) => { if previous_page { navigator.replace(Route::Conventions { query: page_query(page - 1, enabled) }); } else { retry += 1; } }
                        Err(error) => delete_error.set(Some(error.to_string())),
                    }
                });
            }
        }
    }
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

use dioxus::prelude::*;
use domain::dto::InventoryEnableResponse;

use crate::{api, components::ErrorMessage, route::TriggerPending, session};

#[component]
pub fn Triggers() -> Element {
    let TriggerPending(mut pending) = use_context();
    let mut location_id = use_signal(String::new);
    let mut response = use_signal(|| None::<InventoryEnableResponse>);
    let mut error = use_signal(|| None::<String>);
    use_drop(move || pending.set(false));

    rsx! {
        main { class: "page triggers-page", aria_busy: pending(),
            h2 { "Triggers" }
            form { class: "panel trigger-form",
                onsubmit: move |event| {
                    event.prevent_default();
                    if pending() { return; }
                    response.set(None);
                    error.set(None);
                    let location = location_id();
                    if !api::valid_location_id(&location) {
                        error.set(Some("Enter a positive numeric location ID".into()));
                        return;
                    }
                    pending.set(true);
                    spawn(async move {
                        match session::with_session(|| api::inventories::enable_inventory(&location)).await {
                            Ok(result) => response.set(Some(result)),
                            Err(failure) => error.set(Some(failure.to_string())),
                        }
                        pending.set(false);
                        document::eval("requestAnimationFrame(() => document.getElementById('run-trigger')?.focus());");
                    });
                },
                label { r#for: "trigger-action", "Action" }
                select { id: "trigger-action", disabled: pending(),
                    option { value: "product-enablement", "Product enablement" }
                }
                label { r#for: "trigger-location", "Location ID" }
                input { id: "trigger-location", r#type: "text", inputmode: "numeric", required: true, pattern: "[0-9]+", value: location_id(), disabled: pending(),
                    oninput: move |event| location_id.set(event.value()),
                }
                button { id: "run-trigger", class: "primary", r#type: "submit", disabled: pending(), "Run trigger" }
            }
            ErrorMessage { message: error() }
            if let Some(result) = response.read().as_ref() {
                section { class: "panel trigger-result", aria_label: "Trigger result", aria_live: "polite",
                    h3 {
                        if result.failed > 0 { "Completed with errors" }
                        else if result.total == 0 { "No products needed enablement" }
                        else { "Product enablement complete" }
                    }
                    dl { class: "trigger-summary",
                        div { dt { "Processed" } dd { "{result.total}" } }
                        div { dt { "Successful" } dd { "{result.success}" } }
                        div { dt { "Failed" } dd { "{result.failed}" } }
                    }
                    if !result.errors.is_empty() {
                        h4 { "Products that could not be enabled" }
                        ul { class: "trigger-errors",
                            for failure in &result.errors {
                                li {
                                    strong { "Product {failure.product_id}" }
                                    p { "{failure.reason}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

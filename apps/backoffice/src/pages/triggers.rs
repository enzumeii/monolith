use dioxus::prelude::*;
use domain::dto::InventoryEnableResponse;

use crate::{
    api,
    components::{ErrorMessage, LoadingState},
    session,
};

#[component]
pub fn Triggers() -> Element {
    let LoadingState(mut pending) = use_context();
    let mut location_id = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut action = use_trigger(location_id, error, pending, |location: String| async move {
        session::with_session(|| api::inventories::enable_inventory(&location)).await
    });
    use_drop(move || pending.set(false));
    let value = action.value().and_then(Result::ok);
    let outcome = value.as_ref().map(|value| value.read());
    let failure = error().or_else(|| {
        outcome
            .as_deref()
            .and_then(|result| result.as_ref().err())
            .map(ToString::to_string)
    });
    let result = outcome.as_deref().and_then(|result| result.as_ref().ok());

    rsx! {
        main { class: "page triggers-page", aria_busy: pending(),
            h2 { "Triggers" }
            form { class: "panel trigger-form",
                onsubmit: move |event| {
                    event.prevent_default();
                    if pending() { return; }
                    action.reset();
                    error.set(None);
                    let location = location_id();
                    if !api::valid_location_id(&location) {
                        error.set(Some("Enter a positive numeric location ID".into()));
                        return;
                    }
                    pending.set(true);
                    action.call(location);
                },
                label { r#for: "trigger-action", "Action" }
                select { id: "trigger-action", disabled: pending(),
                    option { value: "inventory-enablement", "Inventory enablement" }
                }
                label { r#for: "trigger-location", "Location ID" }
                input { id: "trigger-location", r#type: "text", inputmode: "numeric", required: true, pattern: "[0-9]+", value: location_id(), disabled: pending(),
                    oninput: move |event| location_id.set(event.value()),
                }
                button { id: "run-trigger", class: "primary", r#type: "submit", disabled: pending(), "Run trigger" }
            }
            ErrorMessage { message: failure }
            if let Some(result) = result {
                section { class: "panel trigger-result", aria_label: "Trigger result", aria_live: "polite",
                    h3 {
                        if result.failed > 0 { "Completed with errors" }
                        else if result.total == 0 { "No inventory items needed enablement" }
                        else { "Inventory enablement complete" }
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

type TriggerAction = Action<(String,), api::ApiResult<InventoryEnableResponse>>;

fn use_trigger<
    Fut: std::future::Future<Output = api::ApiResult<InventoryEnableResponse>> + 'static,
>(
    mut location_id: Signal<String>,
    mut error: Signal<Option<String>>,
    mut pending: Signal<bool>,
    load: impl FnMut(String) -> Fut + 'static,
) -> TriggerAction {
    let load = use_callback(load);
    let mut action = use_action(move |location: String| {
        let future = load.call(location);
        async move {
            let result = future.await;
            pending.set(false);
            document::eval(
                "requestAnimationFrame(() => document.getElementById('run-trigger')?.focus());",
            );
            Ok::<_, api::ApiError>(result)
        }
    });
    let mut owner = use_signal(|| {
        session::SESSION
            .peek()
            .as_ref()
            .map(|viewer| (viewer.admin_id, viewer.role))
    });
    use_effect(move || {
        let current = session::SESSION
            .read()
            .as_ref()
            .map(|viewer| (viewer.admin_id, viewer.role));
        if current != *owner.peek() {
            owner.set(current);
            action.reset();
            location_id.set(String::new());
            error.set(None);
            pending.set(false);
        }
    });
    action
}

#[cfg(test)]
#[path = "../tests/pages/triggers.rs"]
mod tests;

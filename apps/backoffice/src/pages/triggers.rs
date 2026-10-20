use dioxus::prelude::*;

use crate::{
    api,
    components::{ErrorMessage, LoadingState},
    session,
};

#[derive(Clone, Copy, PartialEq)]
enum TriggerKind {
    InventoryEnablement,
    ReshipEmail,
}

#[component]
pub fn Triggers() -> Element {
    let LoadingState(mut pending) = use_context();
    let mut location_id = use_signal(String::new);
    let mut order_number = use_signal(String::new);
    let mut kind = use_signal(|| TriggerKind::InventoryEnablement);
    let mut error = use_signal(|| None::<String>);
    let mut action = use_trigger(location_id, error, pending, |location: String| async move {
        session::with_session(|| api::inventories::enable_inventory(&location)).await
    });
    let mut reship = use_trigger(order_number, error, pending, |number: String| async move {
        session::with_session(|| api::orders::reship(&number)).await
    });
    use_drop(move || pending.set(false));
    let inventory_value = action.value().and_then(Result::ok);
    let inventory_outcome = inventory_value.as_ref().map(|value| value.read());
    let reship_value = reship.value().and_then(Result::ok);
    let reship_outcome = reship_value.as_ref().map(|value| value.read());
    let failure = error().or_else(|| {
        if kind() == TriggerKind::InventoryEnablement {
            inventory_outcome
                .as_deref()
                .and_then(|result| result.as_ref().err())
                .map(ToString::to_string)
        } else {
            reship_outcome
                .as_deref()
                .and_then(|result| result.as_ref().err())
                .map(ToString::to_string)
        }
    });
    let inventory_result = inventory_outcome
        .as_deref()
        .and_then(|result| result.as_ref().ok());
    let reship_result = reship_outcome
        .as_deref()
        .and_then(|result| result.as_ref().ok());

    rsx! {
        main { class: "page triggers-page", aria_busy: pending(),
            h2 { "Triggers" }
            form { class: "panel trigger-form",
                onsubmit: move |event| {
                    event.prevent_default();
                    if pending() { return; }
                    error.set(None);
                    match kind() {
                        TriggerKind::InventoryEnablement => {
                            action.reset();
                            reship.reset();
                            let location = location_id();
                            if !api::valid_location_id(&location) {
                                error.set(Some("Enter a positive numeric location ID".into()));
                                return;
                            }
                            pending.set(true);
                            action.call(location);
                        }
                        TriggerKind::ReshipEmail => {
                            action.reset();
                            reship.reset();
                            let number = order_number();
                            if !api::orders::valid_order_number(&number) {
                                error.set(Some("Enter a four-digit order number between 1000 and 9999".into()));
                                return;
                            }
                            pending.set(true);
                            reship.call(number);
                        }
                    }
                },
                label { r#for: "trigger-action", "Action" }
                select { id: "trigger-action", disabled: pending(), value: if kind() == TriggerKind::InventoryEnablement { "inventory-enablement" } else { "reship-email" },
                    onchange: move |event| {
                        kind.set(if event.value() == "reship-email" { TriggerKind::ReshipEmail } else { TriggerKind::InventoryEnablement });
                        action.reset();
                        reship.reset();
                        error.set(None);
                    },
                    option { value: "inventory-enablement", "Inventory enablement" }
                    option { value: "reship-email", "Reship email" }
                }
                if kind() == TriggerKind::InventoryEnablement {
                    label { r#for: "trigger-location", "Location ID" }
                    input { id: "trigger-location", r#type: "text", inputmode: "numeric", required: true, pattern: "[0-9]+", value: location_id(), disabled: pending(),
                        oninput: move |event| location_id.set(event.value()),
                    }
                } else {
                    label { r#for: "trigger-order-number", "Order number" }
                    input { id: "trigger-order-number", r#type: "text", inputmode: "numeric", required: true, minlength: "4", maxlength: "4", pattern: "[0-9]{4}", value: order_number(), disabled: pending(),
                        oninput: move |event| order_number.set(event.value()),
                    }
                }
                button { id: "run-trigger", class: "primary", r#type: "submit", disabled: pending(), "Run trigger" }
            }
            ErrorMessage { message: failure }
            if kind() == TriggerKind::InventoryEnablement {
                if let Some(result) = inventory_result {
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
            } else if let Some(result) = reship_result {
                section { class: "panel trigger-result", aria_label: "Reship email result", aria_live: "polite",
                    h3 { "{reship_result_title(result)}" }
                    if result.success { p { "Email ID: {result.email_id}" } }
                }
            }
        }
    }
}

#[cfg(test)]
type TriggerAction = Action<(String,), api::ApiResult<domain::dto::InventoryEnableResponse>>;

fn use_trigger<T: 'static, Fut: std::future::Future<Output = api::ApiResult<T>> + 'static>(
    mut input: Signal<String>,
    mut error: Signal<Option<String>>,
    mut pending: Signal<bool>,
    load: impl FnMut(String) -> Fut + 'static,
) -> Action<(String,), api::ApiResult<T>> {
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
            input.set(String::new());
            error.set(None);
            pending.set(false);
        }
    });
    action
}

fn reship_result_title(result: &domain::dto::orders::ReshipOrderResponse) -> &'static str {
    if result.success {
        "Reship email sent"
    } else {
        "Reship email failed"
    }
}

#[cfg(test)]
#[path = "../tests/pages/triggers.rs"]
mod tests;

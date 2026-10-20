use dioxus::prelude::*;
use domain::dto::orders::ShippingLabels;
use std::collections::HashMap;

use crate::{
    api::{self, ApiError, ApiResult},
    components::{ErrorMessage, LoadingState},
    pages::reports::{ExportFormat, ReportSkeleton, download, encode_table, encode_table_xlsx},
    session,
};

mod data;
mod export;
use data::{FulfillmentOrder, FulfillmentResult, fulfill_selected, load_fulfillment_orders};
use export::{TableData, fastship_table, fulfillment_table, thai_post_table};

#[derive(Clone, Copy, PartialEq)]
enum OrderView {
    Fulfillment,
    ShippingLabel,
}

enum OrderData {
    Fulfillment(Vec<FulfillmentOrder>),
    ShippingLabels(ShippingLabels),
}

#[derive(Clone)]
struct FulfillmentRequest {
    orders: Vec<FulfillmentOrder>,
    tracking_numbers: HashMap<u64, String>,
}

#[component]
pub fn Orders() -> Element {
    let mut view = use_signal(|| OrderView::Fulfillment);
    let mut submitted = use_signal(|| None::<OrderView>);
    let mut tracking_numbers = use_signal(HashMap::<u64, String>::new);
    let LoadingState(mut overlay_pending) = use_context();
    let mut fulfillment = use_fulfillment_action(overlay_pending);
    let mut report = use_order_report(
        submitted,
        tracking_numbers,
        fulfillment,
        overlay_pending,
        |view| async move { session::with_session(|| generate(view)).await },
    );
    let loading = submitted.read().is_some() && report.pending();
    let value = report.value().and_then(Result::ok);
    let result = value.as_ref().map(|value| value.read());
    let current = result
        .as_deref()
        .filter(|(request, _)| submitted.read().as_ref() == Some(request));
    let data = if loading {
        None
    } else {
        current.and_then(|(_, result)| result.as_ref().ok())
    };
    let report_error = current
        .and_then(|(_, result)| result.as_ref().err())
        .map(ToString::to_string);
    let fulfillment_value = fulfillment.value().and_then(Result::ok);
    let fulfillment_outcome = fulfillment_value.as_ref().map(|value| value.read());
    let fulfillment_results = fulfillment_outcome
        .as_deref()
        .and_then(|result| result.as_ref().ok())
        .cloned();
    let fulfillment_error = fulfillment_outcome
        .as_deref()
        .and_then(|result| result.as_ref().err())
        .map(ToString::to_string);
    let fulfillment_pending = fulfillment.pending();
    let fulfillment_orders = match data {
        Some(OrderData::Fulfillment(orders)) => orders.clone(),
        _ => Vec::new(),
    };
    let has_fulfillment_results = fulfillment_results.is_some();
    let on_fulfill = move |()| {
        if fulfillment_pending || has_fulfillment_results || fulfillment_orders.is_empty() {
            return;
        }
        let request = FulfillmentRequest {
            orders: fulfillment_orders.clone(),
            tracking_numbers: tracking_numbers.read().clone(),
        };
        overlay_pending.set(true);
        fulfillment.call(request);
    };

    rsx! {
        main { class: "page orders-page",
            h2 { "Orders" }
            form { class: "panel report-form",
                onsubmit: move |event| {
                    event.prevent_default();
                    tracking_numbers.write().clear();
                    fulfillment.reset();
                    submitted.set(Some(view()));
                },
                label { r#for: "order-view", "View" }
                select { id: "order-view", value: if view() == OrderView::Fulfillment { "fulfillment" } else { "shipping-label" },
                    disabled: loading || fulfillment_pending,
                    onchange: move |event| {
                        view.set(if event.value() == "shipping-label" { OrderView::ShippingLabel } else { OrderView::Fulfillment });
                        submitted.set(None);
                        tracking_numbers.write().clear();
                        report.reset();
                        fulfillment.reset();
                    },
                    option { value: "fulfillment", "Fulfillment" }
                    option { value: "shipping-label", "Shipping label" }
                }
                button { class: "primary", r#type: "submit", disabled: loading || fulfillment_pending,
                    if loading { "Generating…" } else { "Generate" }
                }
            }
            ErrorMessage { message: report_error }
            ErrorMessage { message: fulfillment_error }
            if loading { ReportSkeleton {} }
            if let Some(OrderData::Fulfillment(orders)) = data {
                FulfillmentTable {
                    orders: orders.clone(),
                    tracking_numbers,
                    results: fulfillment_results.clone(),
                    pending: fulfillment_pending,
                    on_fulfill,
                }
            }
            if let Some(OrderData::ShippingLabels(labels)) = data {
                ShippingLabelTables { labels: labels.clone() }
            }
        }
    }
}

fn use_order_report<Fut: std::future::Future<Output = ApiResult<OrderData>> + 'static>(
    mut submitted: Signal<Option<OrderView>>,
    mut tracking_numbers: Signal<HashMap<u64, String>>,
    mut fulfillment: Action<(FulfillmentRequest,), ApiResult<Vec<FulfillmentResult>>>,
    mut overlay_pending: Signal<bool>,
    load: impl FnMut(OrderView) -> Fut + 'static,
) -> Action<(OrderView,), (OrderView, ApiResult<OrderData>)> {
    let load = use_callback(load);
    let mut action = use_action(move |view: OrderView| {
        let future = load.call(view);
        async move { Ok::<_, ApiError>((view, future.await)) }
    });
    use_effect(move || {
        if let Some(view) = submitted() {
            action.call(view);
        } else {
            action.reset();
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
            submitted.set(None);
            tracking_numbers.write().clear();
            action.reset();
            fulfillment.reset();
            overlay_pending.set(false);
        }
    });
    action
}

fn use_fulfillment_action(
    mut overlay_pending: Signal<bool>,
) -> Action<(FulfillmentRequest,), ApiResult<Vec<FulfillmentResult>>> {
    use_drop(move || overlay_pending.set(false));
    use_action(move |request: FulfillmentRequest| {
        let future = session::with_session(|| {
            fulfill_selected(
                request.orders,
                request.tracking_numbers,
                api::orders::fulfill,
            )
        });
        async move {
            let result = future.await;
            overlay_pending.set(false);
            Ok::<_, ApiError>(result)
        }
    })
}

async fn generate(view: OrderView) -> ApiResult<OrderData> {
    match view {
        OrderView::Fulfillment => load_fulfillment_orders().await.map(OrderData::Fulfillment),
        OrderView::ShippingLabel => api::orders::shipping_labels()
            .await
            .map(OrderData::ShippingLabels),
    }
}

#[component]
fn FulfillmentTable(
    orders: Vec<FulfillmentOrder>,
    tracking_numbers: Signal<HashMap<u64, String>>,
    results: Option<Vec<FulfillmentResult>>,
    pending: bool,
    on_fulfill: EventHandler<()>,
) -> Element {
    let tracking = tracking_numbers.read().clone();
    let table = fulfillment_table(&orders, &tracking, results.as_deref().unwrap_or_default());
    let eligible = orders.iter().any(|order| {
        tracking
            .get(&order.order.id)
            .is_some_and(|value| !value.trim().is_empty())
    });
    let (success, failed) = results.as_ref().map_or((0, 0), |results| {
        let success = results
            .iter()
            .filter(|result| {
                result
                    .result
                    .as_ref()
                    .is_ok_and(|response| response.success)
            })
            .count();
        (success, results.len() - success)
    });
    rsx! {
        section { class: "panel orders-results", aria_label: "Fulfillment orders", aria_busy: pending,
            h3 { "Fulfillment" }
            if orders.is_empty() {
                p { "No orders found." }
            } else {
                div { class: "toolbar report-actions",
                    DownloadTable { table: table.clone(), filename: "fulfillment-orders.csv", sheet_name: "Fulfillment", format: ExportFormat::Csv, id: "fulfillment-orders-save", label: "Save CSV" }
                    button { class: "primary", disabled: pending || results.is_some() || !eligible,
                        onclick: move |_| on_fulfill.call(()),
                        if pending { "Fulfilling…" } else { "Fulfill" }
                    }
                }
                if let Some(results) = &results {
                    p { role: "status", "Fulfillment complete: {success} succeeded, {failed} failed." }
                    ul { class: "fulfillment-results",
                        for result in results {
                            li {
                                strong { "Order {result.order_number}: " }
                                match &result.result {
                                    Ok(response) if response.success => "Fulfilled",
                                    Ok(_) | Err(_) => "Failed",
                                }
                                " — {result.tracking_number}"
                                if let Ok(response) = &result.result {
                                    if !response.tracking_url.is_empty() {
                                        " — "
                                        a { href: "{response.tracking_url}", target: "_blank", rel: "noopener noreferrer", "Tracking link" }
                                    }
                                }
                            }
                        }
                    }
                }
                DataTable { table: table.clone(),
                    tbody {
                        for (order, row) in orders.iter().zip(&table.rows) {
                            {
                                let id = order.order.id;
                                let number = order.order.number;
                                rsx! {
                                    tr { key: "{id}",
                                        for (header, value) in table.headers.iter().zip(row) {
                                            td {
                                                if *header == "tracking_number" {
                                                    input {
                                                        aria_label: "Tracking number for order {number}",
                                                        r#type: "text",
                                                        value: "{value}",
                                                        disabled: pending || results.is_some(),
                                                        oninput: move |event| {
                                                            tracking_numbers.write().insert(id, event.value());
                                                        },
                                                    }
                                                } else {
                                                    "{value}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ShippingLabelTables(labels: ShippingLabels) -> Element {
    let fastship = fastship_table(&labels.fastship);
    let thai_post = thai_post_table(&labels.thaipost);
    rsx! {
        section { class: "panel orders-results", aria_label: "Shipping labels",
            h3 { "Shipping labels" }
            section { class: "shipping-label-group", aria_label: "Fastship labels",
                h4 { "Fastship" }
                if fastship.rows.is_empty() { p { "No record" } }
                else {
                    DownloadTable { table: fastship.clone(), filename: "fastship-labels.csv", sheet_name: "Fastship", format: ExportFormat::Csv, id: "fastship-label-save", label: "Save Fastship CSV" }
                    DataTable { table: fastship }
                }
            }
            section { class: "shipping-label-group", aria_label: "ThaiPost labels",
                h4 { "ThaiPost" }
                if thai_post.rows.is_empty() { p { "No record" } }
                else {
                    DownloadTable { table: thai_post.clone(), filename: "thai-post-labels.xlsx", sheet_name: "ThaiPost", format: ExportFormat::Xlsx, id: "thaipost-label-save", label: "Save ThaiPost XLSX" }
                    DataTable { table: thai_post }
                }
            }
        }
    }
}

#[component]
fn DataTable(table: TableData, children: Option<Element>) -> Element {
    rsx! {
        div { class: "report-scroll", tabindex: "0", role: "region", aria_label: "Order report table",
            table { class: "admins-table report-table",
                thead { tr { for header in table.headers { th { "{header}" } } } }
                if let Some(children) = children {
                    {children}
                } else {
                    tbody {
                        for row in &table.rows {
                            tr { for value in row { td { "{value}" } } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DownloadTable(
    table: TableData,
    filename: String,
    sheet_name: String,
    format: ExportFormat,
    id: String,
    label: String,
) -> Element {
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    rsx! {
        div { class: "order-download",
            button { id: "{id}", class: "ghost", disabled: busy() || table.rows.is_empty(),
                onclick: {
                    let table = table.clone();
                    let filename = filename.clone();
                    let sheet_name = sheet_name.clone();
                    let id = id.clone();
                    move |_| {
                        if busy() || table.rows.is_empty() { return; }
                        busy.set(true);
                        error.set(None);
                        let table = table.clone();
                        let filename = filename.clone();
                        let sheet_name = sheet_name.clone();
                        let id = id.clone();
                        spawn(async move {
                            let bytes = match format {
                                ExportFormat::Csv => Ok(encode_table(table.headers, &table.rows).into_bytes()),
                                ExportFormat::Xlsx => encode_table_xlsx(table.headers, &table.rows, &sheet_name),
                            };
                            let result = match bytes {
                                Ok(bytes) => download(bytes, &filename, format, &id).await,
                                Err(message) => Err(message),
                            };
                            busy.set(false);
                            if let Err(message) = result { error.set(Some(message)); }
                        });
                    }
                },
                if busy() { "Saving…" } else { "{label}" }
            }
            ErrorMessage { message: error() }
        }
    }
}

#[cfg(test)]
#[path = "../tests/pages/orders.rs"]
mod tests;

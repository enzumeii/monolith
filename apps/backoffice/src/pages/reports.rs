mod accounting;
mod csv;
mod export;
mod save;
mod table;

use crate::{api, components::ErrorMessage, session};
use crate::{
    components::ReportSkeleton,
    pagination::{CursorHistory, PageData, collect_pages},
};
use dioxus::prelude::*;
use domain::dto::{InventoryQuery, PageInfo, ProductReportRow};
use save::SaveReport;
use table::ReportTable;

#[derive(Clone, Copy, PartialEq)]
enum ProductReportKind {
    Convention,
    Picklist,
}
#[derive(Clone, PartialEq)]
struct ReportRequest {
    kind: ProductReportKind,
    parameter: String,
    page_size: Option<u16>,
    cursor: Option<String>,
}
struct ReportData {
    products: Vec<ProductReportRow>,
    page_info: Option<PageInfo>,
}

#[component]
pub fn Reports() -> Element {
    let mut kind = use_signal(|| ReportKind::Convention);
    rsx! {
        main { class: "page reports-page",
            h2 { "Reports" }
            div { class: "panel report-form",
                label { r#for: "report-kind", "Report" }
                select { id: "report-kind", value: kind().value(),
                    onchange: move |event| kind.set(match event.value().as_str() {
                        "picklist" => ReportKind::Picklist,
                        "accounting" => ReportKind::Accounting,
                        _ => ReportKind::Convention,
                    }),
                    option { value: "convention", "Convention report" }
                    option { value: "picklist", "Picklist Report" }
                    option { value: "accounting", "Accounting" }
                }
            }
            match kind() {
                ReportKind::Convention => rsx! { ProductReport { key: "{kind().value()}", kind: ProductReportKind::Convention } },
                ReportKind::Picklist => rsx! { ProductReport { key: "{kind().value()}", kind: ProductReportKind::Picklist } },
                ReportKind::Accounting => rsx! { accounting::AccountingReport {} },
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum ReportKind {
    Convention,
    Picklist,
    Accounting,
}

impl ReportKind {
    fn value(self) -> &'static str {
        match self {
            Self::Convention => "convention",
            Self::Picklist => "picklist",
            Self::Accounting => "accounting",
        }
    }
}

#[component]
fn ProductReport(kind: ProductReportKind) -> Element {
    let mut inventory = use_signal(String::new);
    let mut tags = use_signal(String::new);
    let mut page_size = use_signal(|| Some(50_u16));
    let mut history = use_signal(CursorHistory::default);
    let mut submitted = use_signal(|| None::<ReportRequest>);
    let mut local_error = use_signal(|| None::<String>);
    let mut report = use_report(submitted, |request| async move {
        session::with_session(|| generate(request.clone())).await
    });
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
    let error = local_error().or_else(|| {
        if loading {
            None
        } else {
            current
                .and_then(|(_, result)| result.as_ref().err())
                .map(ToString::to_string)
        }
    });
    let can_previous = history.read().has_previous();
    let next_cursor = data
        .and_then(|data| data.page_info.as_ref())
        .map(|info| (info.has_next_page, info.end_cursor.clone()));

    rsx! {
        div {
            form {
                class: "panel report-form",
                onsubmit: move |event| {
                    event.prevent_default();
                    history.write().reset();
                    local_error.set(None);
                    submitted.set(Some(ReportRequest { kind, parameter: if kind == ProductReportKind::Convention { inventory().trim().into() } else { tags().trim().into() }, page_size: page_size(), cursor: None }));
                },
                if kind == ProductReportKind::Convention {
                    label { r#for: "inventory-id", "Inventory ID (Shopify location ID)" }
                    input { id: "inventory-id", required: true, pattern: "[0-9]+", inputmode: "numeric", value: "{inventory}",
                        oninput: move |event| { inventory.set(event.value()); submitted.set(None); history.write().reset(); local_error.set(None); },
                    }
                } else {
                    label { r#for: "report-tags", "Order tags (comma-separated)" }
                    input { id: "report-tags", required: true, value: "{tags}",
                        oninput: move |event| { tags.set(event.value()); submitted.set(None); local_error.set(None); },
                    }
                }
                button { class: "primary", r#type: "submit", disabled: loading, "Generate" }
            }
            ErrorMessage { message: error.clone() }
            if error.is_some() && submitted.read().is_some() {
                button { class: "ghost", onclick: move |_| { local_error.set(None); if let Some(request) = submitted() { report.call(request); } }, "Retry" }
            }
            if loading { ReportSkeleton {} }
            if let Some(data) = data {
                div { class: "toolbar report-actions",
                    p { role: "status", "{data.products.len()} rows" }
                    SaveReport {
                        rows: data.products.clone(),
                        filename: if kind == ProductReportKind::Convention { "convention-report.csv" } else { "picklist-report.csv" },
                    }
                }
                if data.products.is_empty() { p { "No products found." } }
                ReportTable { rows: data.products.clone() }
            }
            if kind == ProductReportKind::Convention {
                div { class: "pager report-pager",
                    label { r#for: "report-page-size", "Rows per page" }
                    select { id: "report-page-size", value: page_size().map_or_else(|| "all".into(), |size| size.to_string()), disabled: loading,
                        onchange: move |event| {
                            let size = event.value().parse::<u16>().ok();
                            page_size.set(size); history.write().reset(); local_error.set(None);
                            if submitted.read().is_some() {
                                submitted.set(Some(ReportRequest { kind: ProductReportKind::Convention, parameter: inventory().trim().into(), page_size: size, cursor: None }));
                            }
                        },
                        option { value: "25", "25" }
                        option { value: "50", "50" }
                        option { value: "100", "100" }
                        option { value: "250", "250" }
                        option { value: "all", "All" }
                    }
                    if page_size().is_some() {
                        button { class: "ghost", disabled: loading || !can_previous,
                            onclick: move |_| {
                                history.write().previous(); local_error.set(None);
                                submitted.with_mut(|request| { if let Some(request) = request { request.cursor.clone_from(&history.read().current); } });
                            }, "Previous" }
                        button { class: "ghost", disabled: loading || !next_cursor.as_ref().is_some_and(|(has_next, _)| *has_next),
                            onclick: move |_| {
                                if let Some((has_next_page, end_cursor)) = next_cursor.clone() {
                                    let advanced = history.write().next(&PageInfo { has_next_page, end_cursor });
                                    match advanced {
                                        Ok(()) => {
                                            local_error.set(None);
                                            submitted.with_mut(|request| { if let Some(request) = request { request.cursor.clone_from(&history.read().current); } });
                                        }
                                        Err(error) => local_error.set(Some(error.to_string())),
                                    }
                                }
                            }, "Next" }
                    }
                }
            }
        }
    }
}

async fn generate(request: ReportRequest) -> api::ApiResult<ReportData> {
    match request.kind {
        ProductReportKind::Picklist => {
            let response = api::reports::get_picklist_report(&request.parameter).await?;
            Ok(ReportData {
                products: response.products,
                page_info: None,
            })
        }
        ProductReportKind::Convention => {
            if let Some(size) = request.page_size {
                let response = api::reports::get_inventory_report(
                    &request.parameter,
                    InventoryQuery {
                        page_size: Some(size),
                        cursor: request.cursor,
                    },
                )
                .await?;
                Ok(ReportData {
                    products: response.products,
                    page_info: Some(response.page_info),
                })
            } else {
                let inventory_id = request.parameter.clone();
                let products = collect_pages(move |cursor| {
                    let inventory_id = inventory_id.clone();
                    async move {
                        let response = api::reports::get_inventory_report(
                            &inventory_id,
                            InventoryQuery {
                                page_size: Some(250),
                                cursor,
                            },
                        )
                        .await?;
                        Ok(PageData::new(response.products, response.page_info))
                    }
                })
                .await?;
                Ok(ReportData {
                    products,
                    page_info: None,
                })
            }
        }
    }
}

fn use_report<Fut: std::future::Future<Output = api::ApiResult<ReportData>> + 'static>(
    mut submitted: Signal<Option<ReportRequest>>,
    load: impl FnMut(ReportRequest) -> Fut + 'static,
) -> Action<(ReportRequest,), (ReportRequest, api::ApiResult<ReportData>)> {
    let load = use_callback(load);
    let mut action = use_action(move |request: ReportRequest| {
        let future = load.call(request.clone());
        async move { Ok::<_, api::ApiError>((request, future.await)) }
    });
    use_effect(move || {
        if let Some(request) = submitted() {
            action.call(request);
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
            action.reset();
        }
    });
    action
}
#[cfg(test)]
#[path = "../tests/pages/reports.rs"]
mod tests;

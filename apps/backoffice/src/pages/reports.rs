mod csv;
mod data;
mod table;

use crate::{api, components::ErrorMessage, session};
use data::{CursorHistory, collect_inventory};
use dioxus::prelude::*;
use domain::dto::{InventoryPageInfo, InventoryQuery, ProductReportRow};
use table::{ReportSkeleton, ReportTable};

#[derive(Clone, Copy, PartialEq)]
enum ReportKind {
    Convention,
    Picklist,
}
#[derive(Clone, PartialEq)]
struct ReportRequest {
    kind: ReportKind,
    parameter: String,
    page_size: Option<u16>,
    cursor: Option<String>,
}
struct ReportData {
    products: Vec<ProductReportRow>,
    page_info: Option<InventoryPageInfo>,
}

#[component]
pub fn Reports() -> Element {
    let mut kind = use_signal(|| ReportKind::Convention);
    let mut inventory = use_signal(String::new);
    let mut tags = use_signal(String::new);
    let mut page_size = use_signal(|| Some(50_u16));
    let mut history = use_signal(CursorHistory::default);
    let mut submitted = use_signal(|| None::<ReportRequest>);
    let mut local_error = use_signal(|| None::<String>);
    let mut report = use_resource(move || async move {
        let request = submitted()?;
        let result = session::with_session(|| generate(request.clone())).await;
        Some((request, result))
    });
    let loading = submitted.read().is_some() && *report.state().read() == UseResourceState::Pending;
    let result = report.read();
    let current = result
        .as_ref()
        .and_then(Option::as_ref)
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
        main { class: "page reports-page",
            h2 { "Reports" }
            form {
                class: "panel report-form",
                onsubmit: move |event| {
                    event.prevent_default();
                    history.write().reset();
                    local_error.set(None);
                    submitted.set(Some(ReportRequest { kind: kind(), parameter: if kind() == ReportKind::Convention { inventory().trim().into() } else { tags().trim().into() }, page_size: page_size(), cursor: None }));
                    report.restart();
                },
                label { r#for: "report-kind", "Report" }
                select { id: "report-kind", value: if kind() == ReportKind::Convention { "convention" } else { "picklist" },
                    onchange: move |event| {
                        kind.set(if event.value() == "picklist" { ReportKind::Picklist } else { ReportKind::Convention });
                        history.write().reset(); submitted.set(None); local_error.set(None);
                    },
                    option { value: "convention", "Convention report" }
                    option { value: "picklist", "Picklist Report" }
                }
                if kind() == ReportKind::Convention {
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
                button { class: "ghost", onclick: move |_| { local_error.set(None); report.restart(); }, "Retry" }
            }
            if loading { ReportSkeleton {} }
            if let Some(data) = data {
                div { class: "toolbar report-actions",
                    p { role: "status", "{data.products.len()} rows" }
                    button { class: "ghost", onclick: {
                        let contents = csv::encode(&data.products);
                        let filename = if kind() == ReportKind::Convention { "convention-report.csv" } else { "picklist-report.csv" };
                        move |_| {
                            let contents = contents.clone();
                            spawn(async move {
                                if let Err(message) = save_csv(contents, filename).await { local_error.set(Some(message)); }
                            });
                        }
                    }, "Save CSV" }
                }
                if data.products.is_empty() { p { "No products found." } }
                ReportTable { rows: data.products.clone() }
            }
            if kind() == ReportKind::Convention {
                div { class: "pager report-pager",
                    label { r#for: "report-page-size", "Rows per page" }
                    select { id: "report-page-size", value: page_size().map_or_else(|| "all".into(), |size| size.to_string()), disabled: loading,
                        onchange: move |event| {
                            let size = event.value().parse::<u16>().ok();
                            page_size.set(size); history.write().reset(); local_error.set(None);
                            if submitted.read().is_some() {
                                submitted.set(Some(ReportRequest { kind: ReportKind::Convention, parameter: inventory().trim().into(), page_size: size, cursor: None }));
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
                                    let advanced = history.write().next(&InventoryPageInfo { has_next_page, end_cursor });
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
        ReportKind::Picklist => {
            let response = api::reports::get_picklist_report(&request.parameter).await?;
            Ok(ReportData {
                products: response.products,
                page_info: None,
            })
        }
        ReportKind::Convention => {
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
                let products = collect_inventory(|cursor| {
                    api::reports::get_inventory_report(
                        &request.parameter,
                        InventoryQuery {
                            page_size: Some(250),
                            cursor,
                        },
                    )
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

async fn save_csv(contents: String, filename: &str) -> Result<(), String> {
    let mut download = document::eval(
        r"
        const [contents, filename] = await dioxus.recv();
        const url = URL.createObjectURL(new Blob([contents], {type: 'text/csv;charset=utf-8'}));
        const link = document.createElement('a');
        link.href = url;
        link.download = filename;
        document.body.appendChild(link);
        link.click();
        link.remove();
        setTimeout(() => URL.revokeObjectURL(url), 1000);
        dioxus.send(true);
    ",
    );
    download
        .send((contents, filename))
        .map_err(|_| "Unable to save CSV".to_string())?;
    download
        .recv::<bool>()
        .await
        .map_err(|_| "Unable to save CSV".to_string())?;
    Ok(())
}

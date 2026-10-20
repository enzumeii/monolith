mod data;
mod export;

use crate::{
    api,
    components::{DataTable, DownloadFile, ErrorMessage, TableData},
    pages::reports::ReportSkeleton,
    session,
};
use data::{ReportRequest, date_range};
use dioxus::prelude::*;
use domain::dto::{AccountingReportQuery, AccountingReportResponse};
use export::{encode_archive, item_table, transaction_table};

type AccountingAction =
    Action<(ReportRequest,), (ReportRequest, api::ApiResult<AccountingReportResponse>)>;

#[component]
pub fn Accounting() -> Element {
    let mut start = use_signal(String::new);
    let mut end = use_signal(String::new);
    let mut submitted = use_signal(|| None::<ReportRequest>);
    let mut local_error = use_signal(|| None::<String>);
    let mut report = use_accounting_report(submitted, |request| async move {
        session::with_session(|| {
            api::accounting::get_report(AccountingReportQuery {
                date_from: request.date_from,
                date_to: request.date_to,
            })
        })
        .await
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
    let csv = transaction_table(
        data.into_iter().flat_map(|data| {
            data.transactions
                .stripe
                .iter()
                .chain(&data.transactions.paypal)
        }),
        "",
    );

    rsx! {
        main { class: "page accounting-page",
            h2 { "Accounting" }
            form { class: "panel report-form",
                onsubmit: move |event| {
                    event.prevent_default();
                    report.reset();
                    match date_range(&start(), &end()) {
                        Ok(request) => { local_error.set(None); submitted.set(Some(request)); }
                        Err(message) => { submitted.set(None); local_error.set(Some(message)); }
                    }
                },
                p { id: "accounting-timezone", "All dates and times are in UTC+07:00." }
                label { r#for: "accounting-start", "Start date and time" }
                input { id: "accounting-start", r#type: "datetime-local", step: "1",
                    required: true, value: "{start}", aria_describedby: "accounting-timezone",
                    oninput: move |event| {
                        start.set(event.value()); submitted.set(None); report.reset(); local_error.set(None);
                    },
                }
                label { r#for: "accounting-end", "End date and time" }
                input { id: "accounting-end", r#type: "datetime-local", step: "1",
                    required: true, value: "{end}", aria_describedby: "accounting-timezone",
                    oninput: move |event| {
                        end.set(event.value()); submitted.set(None); report.reset(); local_error.set(None);
                    },
                }
                button { class: "primary", r#type: "submit", disabled: loading,
                    if loading { "Generating…" } else { "Generate" }
                }
            }
            ErrorMessage { message: error.clone() }
            if error.is_some() && submitted.read().is_some() {
                button { class: "ghost", onclick: move |_| {
                    local_error.set(None);
                    if let Some(request) = submitted() { report.call(request); }
                }, "Retry" }
            }
            AccountingDownload { transactions: csv, items: item_table(data.map_or(&[], |data| data.items.as_slice())) }
            if loading { ReportSkeleton {} }
            if let Some(data) = data {
                AccountingGroup { name: "Stripe", table: transaction_table(&data.transactions.stripe, "null") }
                AccountingGroup { name: "PayPal", table: transaction_table(&data.transactions.paypal, "null") }
                AccountingGroup { name: "Items", table: item_table(&data.items) }
            } else if submitted.read().is_none() && error.is_none() {
                p { "Select a start and end date and time, then generate a report." }
            }
        }
    }
}

#[component]
fn AccountingGroup(name: String, table: TableData) -> Element {
    rsx! {
        section { class: "panel accounting-group", aria_label: "{name} report",
            h3 { "{name}" }
            p { role: "status", "{table.rows.len()} rows" }
            if table.rows.is_empty() { p { "No {name} records found." } }
            DataTable { table, label: format!("{name} report table") }
        }
    }
}

fn use_accounting_report<
    Fut: std::future::Future<Output = api::ApiResult<AccountingReportResponse>> + 'static,
>(
    mut submitted: Signal<Option<ReportRequest>>,
    load: impl FnMut(ReportRequest) -> Fut + 'static,
) -> AccountingAction {
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

#[component]
fn AccountingDownload(transactions: TableData, items: TableData) -> Element {
    let disabled = transactions.rows.is_empty() && items.rows.is_empty();
    rsx! {
        DownloadFile {
            filename: "accounting-report.zip", mime: "application/zip",
            id: "accounting-download", label: "Download consolidated ZIP", disabled,
            encode: move |()| encode_archive(&transactions, &items),
        }
    }
}

#[cfg(test)]
#[path = "../tests/pages/accounting.rs"]
mod tests;

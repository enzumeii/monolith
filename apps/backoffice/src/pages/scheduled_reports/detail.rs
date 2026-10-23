use super::{files::ReportFiles, time::bangkok_time};
use crate::{api, components::ErrorMessage, session};
use dioxus::prelude::*;
use domain::dto::{ReportResultDTO, ReportStatus, ScheduleReportType};

#[component]
pub fn Detail(id: String, query: String) -> Element {
    let (mut load, saved) = use_report(id);
    let loading = load.pending();
    let error = load
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(ToString::to_string);
    rsx! {
        section { class: "panel", aria_label: "Saved report detail", aria_busy: loading,
            Link { to: crate::route::Route::ScheduledReportHistory { query }, "Back to report history" }
            div { class: "schedule-actions",
                button { r#type: "button", class: "ghost", disabled: loading,
                    onclick: move |_| load.restart(), if loading { "Refreshing…" } else { "Refresh" } }
            }
            ErrorMessage { message: error.clone() }
            if error.is_some() {
                button { r#type: "button", class: "ghost", disabled: loading, onclick: move |_| load.restart(), "Retry" }
            }
            if loading && saved.read().is_none() { super::history::HistorySkeleton {} }
            if let Some(record) = saved() {
                dl {
                    dt { "Report ID" } dd { "{record.id}" }
                    dt { "Report type" } dd { "{report_type_label(record.report_type)}" }
                    dt { "Times · Bangkok (UTC+07:00)" } dd { ReportTimes { report: record.clone() } }
                    dt { if record.report_type == ScheduleReportType::Inventory { "Inventory ID" } else { "Parameters" } }
                    dd { "{record.parameters}" }
                    dt { "Workflow status" } dd { ReportOutcome { report: record.clone() } }
                    dt { "Files" } dd { ReportFiles { report_id: record.id, files: record.files } }
                }
            }
        }
    }
}

pub fn use_report(
    id: String,
) -> (
    Resource<api::ApiResult<ReportResultDTO>>,
    Signal<Option<ReportResultDTO>>,
) {
    let mut saved = use_signal(|| None);
    let load = use_resource(move || {
        let id = id.clone();
        async move {
            let result = session::with_session(|| api::report_results::get_report(&id)).await;
            if let Ok(record) = &result {
                saved.set(Some(record.clone()));
            }
            result
        }
    });
    (load, saved)
}

#[component]
pub fn ReportTimes(report: ReportResultDTO) -> Element {
    rsx! {
        p { if let Some(generated) = report.generated_at { "Generated: {bangkok_time(&generated)}" } else { "Not generated" } }
        p { "Attempted: {bangkok_time(&report.attempted_at)}" }
    }
}

#[component]
pub fn ReportOutcome(report: ReportResultDTO) -> Element {
    let value = outcome(&report);
    rsx! {
        p { strong { "{value.label}" } " · {value.explanation}" }
        if let Some(error) = value.error { p { class: "schedule-field-error", "{error}" } }
    }
}

#[derive(Debug, PartialEq)]
pub struct Outcome<'a> {
    pub label: &'static str,
    pub explanation: &'static str,
    pub error: Option<&'a str>,
}

pub fn outcome(report: &ReportResultDTO) -> Outcome<'_> {
    let (label, explanation) = match report.status {
        ReportStatus::Pending => ("PENDING", "Workflow in progress or retrying"),
        ReportStatus::Success => (
            "SUCCESS",
            "Workflow completed; email delivery is not confirmed by this status",
        ),
        ReportStatus::Failed => ("FAILED", "Workflow failed"),
    };
    Outcome {
        label,
        explanation,
        error: report.error.as_deref(),
    }
}

pub fn report_type_label(kind: ScheduleReportType) -> &'static str {
    match kind {
        ScheduleReportType::Transaction => "Transactions",
        ScheduleReportType::Inventory => "Inventory",
    }
}

#[cfg(test)]
#[path = "../../tests/pages/scheduled_reports/detail.rs"]
mod tests;

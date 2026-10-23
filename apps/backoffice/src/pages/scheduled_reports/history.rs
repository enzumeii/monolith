use super::{
    detail::{ReportOutcome, ReportTimes, report_type_label},
    files::ReportFiles,
    time,
};
use crate::{
    api,
    components::{DataTable, DateTimePicker, ErrorMessage, TableData},
    route::Route,
    session,
};
use dioxus::prelude::*;
use domain::dto::{ListReportResultsQuery, ScheduleReportType};

#[component]
pub fn History(query: String) -> Element {
    let filters = HistoryFilters::parse(&query);
    let (mut load, saved) = use_history(filters.clone());
    let mut draft_type = use_signal(|| match filters.report_type {
        Some(ScheduleReportType::Transaction) => "TRANSACTION".to_string(),
        Some(ScheduleReportType::Inventory) => "INVENTORY".to_string(),
        None => String::new(),
    });
    let mut draft_start = use_signal(|| {
        filters
            .generated_from
            .as_deref()
            .map(time::local_control)
            .unwrap_or_default()
    });
    let mut draft_end = use_signal(|| {
        filters
            .generated_to
            .as_deref()
            .map(time::local_control)
            .unwrap_or_default()
    });
    let mut draft_size = use_signal(|| filters.per_page);
    let mut validation = use_signal(|| None::<String>);
    let navigator = use_navigator();
    let loading = load.pending();
    let error = validation().or_else(|| {
        load.read()
            .as_ref()
            .and_then(|result| result.as_ref().err())
            .map(ToString::to_string)
    });
    let page = saved();
    let mut previous = filters.clone();
    previous.page = previous.page.saturating_sub(1).max(1);
    let mut next = filters.clone();
    next.page = next.page.saturating_add(1);
    let has_next = page.as_ref().is_some_and(|page| page.has_next);
    let filtered = filters.report_type.is_some()
        || filters.generated_from.is_some()
        || filters.generated_to.is_some();
    rsx! {
        section { aria_label: "Global saved report history", aria_busy: loading,
            p { "Global saved reports. Date filters use generated time, or attempted time when not generated. All times are Bangkok (UTC+07:00)." }
            form { class: "panel schedule-filters", novalidate: true,
                onsubmit: move |event| {
                    event.prevent_default();
                    match HistoryFilters::apply(&draft_type(), &draft_start(), &draft_end(), draft_size()) {
                        Ok(filters) => { validation.set(None); navigator.push(Route::ScheduledReportHistory { query: filters.query() }); },
                        Err(error) => validation.set(Some(error)),
                    }
                },
                div {
                    label { r#for: "history-type", "Report type" }
                    select { id: "history-type", value: draft_type(), onchange: move |event| draft_type.set(event.value()),
                        option { value: "", "All types" }
                        option { value: "TRANSACTION", "Transactions" }
                        option { value: "INVENTORY", "Inventory" }
                    }
                }
                div {
                    DateTimePicker { id: "history-from", label: "Generated / attempted start · inclusive · Bangkok", value: draft_start(),
                        describedby: "history-bounds-help history-filter-error", invalid: validation().is_some(),
                        on_input: move |value| draft_start.set(value) }
                }
                div {
                    DateTimePicker { id: "history-to", label: "Generated / attempted end · exclusive · Bangkok", value: draft_end(),
                        describedby: "history-bounds-help history-filter-error", invalid: validation().is_some(),
                        on_input: move |value| draft_end.set(value) }
                }
                div {
                    label { r#for: "history-size", "Per page" }
                    select { id: "history-size", value: "{draft_size}", onchange: move |event| draft_size.set(event.value().parse().unwrap_or(25)),
                        if ![10,25,50,100].contains(&draft_size()) { option { value: "{draft_size}", "{draft_size}" } }
                        option { value: "10", "10" } option { value: "25", "25" }
                        option { value: "50", "50" } option { value: "100", "100" }
                    }
                }
                p { id: "history-bounds-help", "Either date bound may be empty. Start is included; end is excluded." }
                div { id: "history-filter-error", role: "alert", if let Some(message) = validation() { "{message}" } }
                div { class: "schedule-actions",
                    button { r#type: "submit", class: "primary", "Apply filters" }
                    Link { to: Route::ScheduledReportHistory { query: "page=1&per_page=25".into() }, "Clear filters" }
                    button { r#type: "button", class: "ghost", disabled: loading,
                        onclick: move |_| load.restart(), if loading { "Refreshing…" } else { "Refresh" } }
                }
            }
            ErrorMessage { message: error.clone() }
            if error.is_some() {
                button { r#type: "button", class: "ghost", disabled: loading, onclick: move |_| load.restart(), "Retry" }
            }
            if loading {
                if page.is_none() { HistorySkeleton {} }
                else { p { role: "status", "Refreshing reports… Previous results remain visible." } }
            }
            if let Some(page) = page {
                if page.items.is_empty() {
                    p { role: "status",
                        if filters.page > 1 { "No reports on this page. Use Previous to return." }
                        else if filtered { "No reports match these filters." }
                        else { "No saved reports yet." }
                    }
                } else {
                    DataTable {
                        table: TableData { headers: &["Report ID / type", "Generated / attempted", "Parameters", "Workflow status", "Files"], rows: vec![] },
                        label: "Saved reports", caption: "Global saved reports · Bangkok (UTC+07:00)", class: "schedule-table",
                            tbody {
                                for record in &page.items {
                                    tr { key: "{record.id}",
                                        td {
                                            Link { to: Route::ScheduledReportDetail { id: record.id.clone(), query: query.clone() }, "{record.id}" }
                                            p { "{report_type_label(record.report_type)}" }
                                        }
                                        td { ReportTimes { report: record.clone() } }
                                        td {
                                            if record.report_type == ScheduleReportType::Inventory { "Inventory ID: " }
                                            "{record.parameters}"
                                        }
                                        td { ReportOutcome { report: record.clone() } }
                                        td { ReportFiles { report_id: record.id.clone(), files: record.files.clone(), compact: true } }
                                    }
                                }
                            }
                    }
                }
            }
            nav { class: "schedule-pagination", aria_label: "Report history pagination",
                button { r#type: "button", class: "ghost", disabled: loading || filters.page <= 1,
                    onclick: move |_| { navigator.push(Route::ScheduledReportHistory { query: previous.query() }); }, "Previous" }
                span { role: "status", "Page {filters.page}" }
                button { r#type: "button", class: "ghost", disabled: loading || !has_next,
                    onclick: move |_| { navigator.push(Route::ScheduledReportHistory { query: next.query() }); }, "Next" }
            }
        }
    }
}

pub fn use_history(
    filters: HistoryFilters,
) -> (
    Resource<api::ApiResult<api::report_results::ReportResultListResponse>>,
    Signal<Option<api::report_results::ReportResultListResponse>>,
) {
    let mut saved = use_signal(|| None);
    let load = use_resource(move || {
        let request = filters.request();
        async move {
            let request = request
                .map_err(|error| api::ApiError::new(reqwest::StatusCode::BAD_REQUEST, error))?;
            let result = session::with_session(|| api::report_results::list_reports(request)).await;
            if let Ok(page) = &result {
                saved.set(Some(page.clone()));
            }
            result
        }
    });
    (load, saved)
}

#[component]
pub fn HistorySkeleton() -> Element {
    rsx! {
        div { role: "status", aria_live: "polite", aria_busy: "true",
            p { "Loading saved reports…" }
            div { class: "report-skeleton", aria_hidden: "true", for _ in 0..5 { div { class: "report-skeleton-row" } } }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistoryFilters {
    pub report_type: Option<ScheduleReportType>,
    pub generated_from: Option<String>,
    pub generated_to: Option<String>,
    pub page: u32,
    pub per_page: u32,
}
impl HistoryFilters {
    pub fn parse(query: &str) -> Self {
        let mut result = Self {
            report_type: None,
            generated_from: None,
            generated_to: None,
            page: 1,
            per_page: 25,
        };
        let mut url = reqwest::Url::parse("https://backoffice.invalid/").expect("static URL");
        let query = query.trim_start_matches('?').replace('+', "%2B");
        url.set_query(Some(&query));
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "report_type" => result.report_type = parse_type(&value),
                "generated_from" => {
                    result.generated_from = (!value.is_empty()).then(|| value.into_owned());
                }
                "generated_to" => {
                    result.generated_to = (!value.is_empty()).then(|| value.into_owned());
                }
                "page" => result.page = value.parse().ok().filter(|page| *page > 0).unwrap_or(1),
                "per_page" => {
                    result.per_page = value
                        .parse()
                        .ok()
                        .filter(|size| (1..=100).contains(size))
                        .unwrap_or(25);
                }
                _ => {}
            }
        }
        result
    }
    pub fn query(&self) -> String {
        let mut url = reqwest::Url::parse("https://backoffice.invalid/").expect("static URL");
        {
            let mut query = url.query_pairs_mut();
            if let Some(kind) = self.report_type {
                query.append_pair(
                    "report_type",
                    match kind {
                        ScheduleReportType::Inventory => "INVENTORY",
                        ScheduleReportType::Transaction => "TRANSACTION",
                    },
                );
            }
            if let Some(value) = &self.generated_from {
                query.append_pair("generated_from", value);
            }
            if let Some(value) = &self.generated_to {
                query.append_pair("generated_to", value);
            }
            query.append_pair("page", &self.page.to_string());
            query.append_pair("per_page", &self.per_page.to_string());
        }
        url.query().unwrap_or_default().into()
    }
    pub fn request(&self) -> Result<ListReportResultsQuery, String> {
        time::validate_bounds(self.generated_from.as_deref(), self.generated_to.as_deref())?;
        Ok(ListReportResultsQuery {
            report_type: self.report_type,
            generated_from: self.generated_from.clone(),
            generated_to: self.generated_to.clone(),
            page: Some(self.page),
            per_page: Some(self.per_page),
        })
    }
    pub fn apply(report_type: &str, start: &str, end: &str, per_page: u32) -> Result<Self, String> {
        let result = Self {
            report_type: parse_type(report_type),
            generated_from: time::local_bound(start)?,
            generated_to: time::local_bound(end)?,
            page: 1,
            per_page: per_page.clamp(1, 100),
        };
        result.request()?;
        Ok(result)
    }
}

fn parse_type(value: &str) -> Option<ScheduleReportType> {
    match value {
        "INVENTORY" => Some(ScheduleReportType::Inventory),
        "TRANSACTION" => Some(ScheduleReportType::Transaction),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/pages/scheduled_reports/history.rs"]
mod tests;

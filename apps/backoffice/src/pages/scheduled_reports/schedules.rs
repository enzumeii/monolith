use super::{
    data::cadence_label,
    editor::ScheduleEditor,
    state::{Filters, SaveRecovery},
};
use crate::{
    api,
    components::{Confirm, DataTable, ErrorMessage, TableData},
    route::Route,
    session,
};
use dioxus::prelude::*;
use domain::dto::{CronConfigurationDTO, PatchCronConfiguration, PatchField, ScheduleReportType};

#[derive(Clone, Debug, PartialEq)]
enum EditorSelection {
    New,
    Existing(String),
}
impl EditorSelection {
    fn id(&self) -> Option<String> {
        match self {
            Self::New => None,
            Self::Existing(id) => Some(id.clone()),
        }
    }
}

#[component]
pub fn Schedules(query: String) -> Element {
    let owner = super::state::use_schedule_owner();
    let filters = Filters::parse(&query);
    let mut refresh = use_signal(|| 0_u64);
    let mut editor = use_signal(|| None::<EditorSelection>);
    let mut deleting = use_signal(|| None::<CronConfigurationDTO>);
    let mut mutating = use_signal(|| false);
    let mut recovery = use_signal(SaveRecovery::default);
    let blocked_refresh = use_signal(|| None::<u64>);
    let list = use_resource(use_reactive((&query,), move |(query,)| {
        let _owner = owner();
        let refresh_version = refresh();
        let request = Filters::parse(&query).request();
        async move {
            let result =
                session::with_session(|| api::cron_configurations::list_configurations(request))
                    .await;
            (query, refresh_version, result)
        }
    }));
    use_effect(move || {
        if let Some((_, version, result)) = list.read().as_ref() {
            complete_list_reload(recovery, blocked_refresh, *version, result);
        }
    });
    let navigator = use_navigator();
    let response = list
        .read()
        .as_ref()
        .filter(|(snapshot, _, _)| snapshot == &query)
        .map(|(_, _, result)| result.clone());
    let loading = list.pending() || response.is_none();
    let page = if loading {
        None
    } else {
        response.as_ref().and_then(|value| value.as_ref().ok())
    };
    let error = recovery()
        .failure
        .map(|value| value.to_string())
        .or_else(|| {
            response
                .as_ref()
                .and_then(|value| value.as_ref().err())
                .map(ToString::to_string)
        });
    let busy = mutating();
    let filter_type = filters.clone();
    let filter_status = filters.clone();
    let filter_size = filters.clone();
    let mut previous = filters.clone();
    previous.page = previous.page.saturating_sub(1).max(1);
    let mut next = filters.clone();
    next.page = next.page.saturating_add(1);
    let has_next = page.is_some_and(|page| page.has_next);
    let editing = editor().is_some();

    rsx! {
        section { aria_label: "Schedules",
            p { "Schedules run in Asia/Bangkok. Enable or disable affects future dispatch; existing queued or running jobs remain." }
            div { class: "panel schedule-filters", aria_busy: loading,
                div {
                    label { r#for: "schedule-filter-type", "Report type" }
                    select { id: "schedule-filter-type", disabled: loading || busy,
                        value: match filters.report_type { Some(ScheduleReportType::Transaction) => "TRANSACTION", Some(ScheduleReportType::Inventory) => "INVENTORY", None => "" },
                        onchange: move |event| { navigator.push(Route::ScheduledReports { query: filter_type.with_type(&event.value()).query() }); },
                        option { value: "", "All types" }
                        option { value: "TRANSACTION", "Transactions" }
                        option { value: "INVENTORY", "Inventory" }
                    }
                }
                div {
                    label { r#for: "schedule-filter-enabled", "Status" }
                    select { id: "schedule-filter-enabled", disabled: loading || busy,
                        value: match filters.enabled { Some(true) => "true", Some(false) => "false", None => "" },
                        onchange: move |event| { let mut next = filter_status.clone(); next.enabled = event.value().parse().ok(); next.page = 1; navigator.push(Route::ScheduledReports { query: next.query() }); },
                        option { value: "", "All statuses" }
                        option { value: "true", "Enabled" }
                        option { value: "false", "Disabled" }
                    }
                }
                div {
                    label { r#for: "schedule-filter-size", "Per page" }
                    select { id: "schedule-filter-size", disabled: loading || busy, value: "{filters.per_page}",
                        onchange: move |event| { let mut next = filter_size.clone(); next.per_page = event.value().parse().unwrap_or(25); next.page = 1; navigator.push(Route::ScheduledReports { query: next.query() }); },
                        if ![10, 25, 50, 100].contains(&filters.per_page) { option { value: "{filters.per_page}", "{filters.per_page}" } }
                        option { value: "10", "10" } option { value: "25", "25" } option { value: "50", "50" } option { value: "100", "100" }
                    }
                }
                div { class: "schedule-actions",
                    button { r#type: "button", class: "ghost", disabled: loading || busy,
                        onclick: move |_| { recovery.write().edited(); refresh += 1; }, "Refresh" }
                    button { r#type: "button", class: "primary", disabled: busy || editing,
                        onclick: move |_| editor.set(Some(EditorSelection::New)), "Create schedule" }
                }
            }
            ErrorMessage { message: error }
            if recovery.read().reload_required {
                p { "The schedule changed. Refresh the list and try again." }
            }
            if loading { p { role: "status", "Loading schedules…" } }
            if let Some(page) = page {
                if page.items.is_empty() {
                    p { role: "status", if filters.page > 1 { "No schedules on this page. Use Previous to return." } else { "No schedules yet" } }
                    if filters.report_type.is_some() || filters.enabled.is_some() {
                        p { "No schedules match these filters." }
                        button { r#type: "button", class: "ghost", onclick: move |_| { navigator.push(Route::ScheduledReports { query: Filters::default().query() }); }, "Clear filters" }
                    }
                } else {
                    DataTable {
                        table: TableData { headers: &["Name", "Report", "Parameters", "Cadence", "Receiver", "Status", "Actions"], rows: vec![] },
                        label: "Saved schedules", caption: "Saved schedules · all times in Bangkok", class: "schedule-table",
                            tbody {
                                for record in &page.items {
                                    ScheduleRow { key: "{record.id}", record: record.clone(), disabled: busy || loading || editing || recovery.read().reload_required,
                                        on_edit: move |id| { if !mutating() && !recovery.read().reload_required && editor().is_none() { editor.set(Some(EditorSelection::Existing(id))); } },
                                        on_delete: move |record| { if !mutating() && !recovery.read().reload_required && editor().is_none() { deleting.set(Some(record)); } },
                                        on_toggle: move |record: CronConfigurationDTO| {
                                            if !begin_row_mutation(editor, mutating, recovery) { return; }
                                            recovery.write().edited();
                                            spawn(async move {
                                                let patch = PatchCronConfiguration { enabled: PatchField::Set(!record.enabled), ..Default::default() };
                                                let result = session::with_session(|| api::cron_configurations::update_configuration(&record.id, patch)).await;
                                                mutating.set(false);
                                                match result { Ok(_) => { refresh += 1; }, Err(error) => record_row_failure(recovery, blocked_refresh, refresh(), error) }
                                            });
                                        },
                                    }
                                }
                            }
                    }
                }
            }
            div { class: "schedule-pagination", aria_label: "Schedule pagination",
                button { r#type: "button", class: "ghost", disabled: loading || busy || filters.page <= 1,
                    onclick: move |_| { navigator.push(Route::ScheduledReports { query: previous.query() }); }, "Previous" }
                span { role: "status", "Page {filters.page}" }
                button { r#type: "button", class: "ghost", disabled: loading || busy || !has_next,
                    onclick: move |_| { navigator.push(Route::ScheduledReports { query: next.query() }); }, "Next" }
            }
            if let Some(selection) = editor() {
                ScheduleEditor { key: "{selection:?}", id: selection.id(),
                    on_close: move |()| editor.set(None),
                    on_saved: move |()| { editor.set(None); recovery.write().edited(); refresh += 1; },
                }
            }
        }
        Confirm {
            open: deleting().is_some(),
            title: deleting().map(|record| format!("Delete ‘{}’?", record.name)).unwrap_or_default(),
            message: "Delete this schedule? Saved reports remain available. Queued jobs are removed and incomplete audits are marked cancelled. An in-flight email may already have been sent.",
            confirm_label: "Delete schedule", danger: true,
            on_cancel: move |()| deleting.set(None),
            on_confirm: move |()| {
                let Some(record) = deleting() else { return; };
                if !begin_row_mutation(editor, mutating, recovery) { return; }
                deleting.set(None); recovery.write().edited();
                spawn(async move {
                    let result = session::with_session(|| api::cron_configurations::delete_configuration(&record.id)).await;
                    mutating.set(false);
                    match result { Ok(()) => { refresh += 1; }, Err(error) => record_row_failure(recovery, blocked_refresh, refresh(), error) }
                });
            },
        }
    }
}

#[component]
fn ScheduleRow(
    record: CronConfigurationDTO,
    disabled: bool,
    on_edit: EventHandler<String>,
    on_delete: EventHandler<CronConfigurationDTO>,
    on_toggle: EventHandler<CronConfigurationDTO>,
) -> Element {
    let id = record.id.clone();
    let toggle = record.clone();
    let remove = record.clone();
    rsx! {
        tr {
            td { "{record.name}" }
            td { if record.report_type == ScheduleReportType::Inventory { "Inventory" } else { "Transactions" } }
            td {
                if record.report_type == ScheduleReportType::Inventory { "Inventory ID: {record.parameter}" }
                else if record.parameter == "FIRST_HALF_MONTH" { "Days 1–15 of current month" }
                else { "Previous calendar month" }
            }
            td { "{cadence_label(&record)}" }
            td { "{record.receiver_email}" }
            td { if record.enabled { "Enabled" } else { "Disabled" } }
            td { div { class: "schedule-actions",
                button { r#type: "button", class: "ghost", disabled, onclick: move |_| on_edit(id.clone()), "Edit" }
                button { r#type: "button", class: "ghost", disabled, aria_label: format!("{} {}", if record.enabled { "Disable" } else { "Enable" }, record.name),
                    onclick: move |_| on_toggle(toggle.clone()), if record.enabled { "Disable" } else { "Enable" } }
                button { r#type: "button", class: "danger", disabled, aria_label: format!("Delete {}", record.name), onclick: move |_| on_delete(remove.clone()), "Delete" }
            } }
        }
    }
}

fn begin_row_mutation(
    editor: Signal<Option<EditorSelection>>,
    mut pending: Signal<bool>,
    recovery: Signal<SaveRecovery>,
) -> bool {
    if editor.read().is_some() || pending() || recovery.read().reload_required {
        return false;
    }
    pending.set(true);
    true
}

fn record_row_failure(
    mut recovery: Signal<SaveRecovery>,
    mut blocked_refresh: Signal<Option<u64>>,
    refresh_version: u64,
    error: api::ApiError,
) {
    if error.status == 409 {
        blocked_refresh.set(Some(refresh_version));
    }
    recovery.write().save_failed(error);
}

fn complete_list_reload(
    mut recovery: Signal<SaveRecovery>,
    mut blocked_refresh: Signal<Option<u64>>,
    refresh_version: u64,
    result: &api::ApiResult<domain::dto::CronConfigurationListResponse>,
) {
    if result.is_ok()
        && blocked_refresh
            .peek()
            .is_none_or(|blocked| refresh_version > blocked)
    {
        recovery.write().loaded();
        blocked_refresh.set(None);
    }
}

#[cfg(test)]
#[path = "../../tests/pages/scheduled_reports/schedules.rs"]
mod tests;

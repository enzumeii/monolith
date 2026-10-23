use super::{
    data::{Cadence, FieldError, ScheduleForm, backend_field, weekday_label},
    state::{RequestState, SaveRecovery},
    time::bangkok_time,
};
use crate::{
    api,
    components::{Confirm, ErrorMessage, Popup, TimePicker},
    session,
};
use dioxus::prelude::*;
use domain::dto::{CronConfigurationDTO, CronConfigurationPreview, ScheduleReportType};

#[component]
pub fn ScheduleEditor(
    id: Option<String>,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
) -> Element {
    let owner = super::state::use_schedule_owner();
    let mut reload = use_signal(|| 0_u64);
    let detail_id = id.clone();
    let detail = use_resource(move || {
        let _owner = owner();
        let _reload = reload();
        let id = detail_id.clone();
        async move {
            match id {
                Some(id) => {
                    session::with_session(|| api::cron_configurations::get_configuration(&id))
                        .await
                        .map(Some)
                }
                None => Ok(None),
            }
        }
    });
    let mut form = use_signal(ScheduleForm::default);
    let mut original = use_signal(ScheduleForm::default);
    let mut errors = use_signal(Vec::<FieldError>::new);
    let mut recovery = use_signal(SaveRecovery::default);
    let mut saving = use_signal(|| false);
    let mut preview_state = use_signal(RequestState::default);
    let mut preview = use_signal(|| None::<CronConfigurationPreview>);
    let mut dismiss = use_signal(|| false);
    let mut confirm_reload = use_signal(|| false);
    use_effect(move || {
        if let Some(Ok(record)) = &*detail.read() {
            let next = record
                .as_ref()
                .map(ScheduleForm::from_record)
                .unwrap_or_default();
            form.set(next.clone());
            original.set(next);
            errors.set(Vec::new());
            recovery.write().loaded();
            preview.set(None);
            preview_state.write().edited();
        }
    });
    let change = use_callback(move |next: ScheduleForm| {
        form.set(next);
        errors.set(Vec::new());
        recovery.write().edited();
        preview.set(None);
        preview_state.write().edited();
    });
    let loading = detail.read().is_none() || detail.pending();
    let load_error = detail
        .read()
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(ToString::to_string);
    let current = form();
    let dirty = current != original();
    let busy = saving();
    let save_id = id.clone();
    let is_edit = id.is_some();
    let summary = recovery()
        .failure
        .map(|error| error.to_string())
        .or_else(|| {
            (!errors.read().is_empty()).then(|| "Check the marked fields before continuing.".into())
        });

    rsx! {
        Popup { open: true, title: if is_edit { "Edit schedule" } else { "Create schedule" }, close_disabled: busy,
            on_close: move |()| { if form() == original() { on_close(()); } else { dismiss.set(true); } },
            section { class: "schedule-editor", aria_busy: loading || busy,
            p { "Every edit, including name or receiver email, starts a future revision. Enable or disable affects future dispatch; existing queued or running jobs remain." }
            if loading { p { role: "status", "Loading schedule…" } }
            ErrorMessage { message: load_error.clone() }
            if load_error.is_some() {
                button { r#type: "button", class: "ghost", onclick: move |_| { reload += 1; }, "Retry loading" }
            }
            if !loading && load_error.is_none() {
                form { novalidate: true,
                    onsubmit: move |event| {
                        event.prevent_default();
                        if saving() || recovery.read().reload_required { return; }
                        let snapshot = form();
                        if let Err(validation) = snapshot.create() { errors.set(validation); return; }
                        saving.set(true);
                        recovery.write().edited();
                        let id = save_id.clone();
                        spawn(async move {
                            let result: api::ApiResult<CronConfigurationDTO> = if let Some(id) = id {
                                let input = snapshot.patch().expect("validated snapshot");
                                session::with_session(|| api::cron_configurations::update_configuration(&id, input)).await
                            } else {
                                let input = snapshot.create().expect("validated snapshot");
                                session::with_session(|| api::cron_configurations::create_configuration(input)).await
                            };
                            saving.set(false);
                            match result {
                                Ok(_) => on_saved(()),
                                Err(error) => {
                                    errors.set(vec![FieldError { field: backend_field(&error.to_string()), message: error.to_string() }]);
                                    recovery.write().save_failed(error);
                                }
                            }
                        });
                    },
                    fieldset { disabled: busy,
                        label { r#for: "schedule-name", "Name" }
                        input { id: "schedule-name", autofocus: true, maxlength: "120", required: true, value: current.name.clone(),
                            onmounted: move |event| async move { let _ = event.data().set_focus(true).await; },
                            aria_invalid: has_error(errors, "name"), aria_describedby: "schedule-name-error",
                            oninput: move |event| { let mut next = form(); next.name = event.value(); change.call(next); },
                        }
                        FieldMessage { errors: errors(), field: "name" }
                        label { r#for: "schedule-email", "Receiver email" }
                        input { id: "schedule-email", r#type: "email", required: true, value: current.email.clone(),
                            aria_invalid: has_error(errors, "email"), aria_describedby: "schedule-email-error",
                            oninput: move |event| { let mut next = form(); next.email = event.value(); change.call(next); },
                        }
                        FieldMessage { errors: errors(), field: "email" }
                        label { r#for: "schedule-type", "Report type" }
                        select { id: "schedule-type", value: if current.report_type == ScheduleReportType::Inventory { "INVENTORY" } else { "TRANSACTION" },
                            onchange: move |event| {
                                let mut next = form(); next.switch_type(if event.value() == "INVENTORY" { ScheduleReportType::Inventory } else { ScheduleReportType::Transaction }); change.call(next);
                            },
                            option { value: "TRANSACTION", "Transactions" }
                            option { value: "INVENTORY", "Inventory" }
                        }
                        if current.report_type == ScheduleReportType::Inventory {
                            p { "Snapshot of inventory when this schedule runs." }
                            label { r#for: "schedule-parameter", "Inventory location ID" }
                            input { id: "schedule-parameter", inputmode: "numeric", required: true, value: current.parameter.clone(),
                                aria_invalid: has_error(errors, "parameter"), aria_describedby: "schedule-parameter-error",
                                oninput: move |event| { let mut next = form(); next.parameter = event.value(); change.call(next); },
                            }
                        } else {
                            label { r#for: "schedule-parameter", "Transaction period policy" }
                            select { id: "schedule-parameter", value: current.parameter.clone(),
                                aria_invalid: has_error(errors, "parameter"), aria_describedby: "schedule-parameter-error",
                                onchange: move |event| {
                                    let mut next = form(); next.parameter = event.value();
                                    if next.cadence != Cadence::Custom { next.cadence = if next.parameter == "FIRST_HALF_MONTH" { Cadence::FirstHalf } else { Cadence::PreviousMonth }; }
                                    change.call(next);
                                },
                                option { value: "PREVIOUS_MONTH", "Previous calendar month · run on day 1" }
                                option { value: "FIRST_HALF_MONTH", "Days 1–15 of current month · run on day 16" }
                            }
                        }
                        FieldMessage { errors: errors(), field: "parameter" }
                        label { r#for: "schedule-cadence", "Recurrence" }
                        {rsx! {
                        select { key: "{current.report_type:?}", id: "schedule-cadence", value: cadence_value(current.cadence),
                            onchange: move |event| {
                                let mut next = form();
                                next.set_cadence(match event.value().as_str() { "previous" => Cadence::PreviousMonth, "first-half" => Cadence::FirstHalf, "daily" => Cadence::Daily, "weekly" => Cadence::Weekly, "monthly" => Cadence::Monthly, _ => Cadence::Custom });
                                change.call(next);
                            },
                            if current.report_type == ScheduleReportType::Transaction {
                                option { value: "previous", "Monthly · previous month" }
                                option { value: "first-half", "Monthly · first half" }
                            } else {
                                option { value: "daily", "Daily" }
                                option { value: "weekly", "Weekly" }
                                option { value: "monthly", "Monthly" }
                            }
                            option { value: "custom", "Custom cron" }
                        }
                        }}
                        if current.cadence == Cadence::Weekly {
                            label { r#for: "schedule-weekday", "Weekday · Bangkok" }
                            select { id: "schedule-weekday", value: current.weekday.clone(), aria_invalid: has_error(errors, "weekday"), aria_describedby: "schedule-weekday-error",
                                onchange: move |event| { let mut next = form(); next.weekday = event.value(); change.call(next); },
                                for day in 0..=6 { option { value: "{day}", "{weekday_label(&day.to_string())}" } }
                            }
                            FieldMessage { errors: errors(), field: "weekday" }
                        }
                        if current.cadence == Cadence::Monthly {
                            label { r#for: "schedule-month-day", "Day of month · Bangkok" }
                            input { id: "schedule-month-day", r#type: "number", min: "1", max: "31", required: true, value: current.month_day.clone(), aria_invalid: has_error(errors, "month-day"), aria_describedby: "schedule-month-day-error schedule-month-day-help",
                                oninput: move |event| { let mut next = form(); next.month_day = event.value(); change.call(next); },
                            }
                            p { id: "schedule-month-day-help", "Months without the selected day are skipped." }
                            FieldMessage { errors: errors(), field: "month-day" }
                        }
                        if current.cadence == Cadence::Custom {
                            label { r#for: "schedule-cron", "Five-field cron expression · Asia/Bangkok" }
                            input { id: "schedule-cron", required: true, value: current.custom.clone(),
                                aria_invalid: has_error(errors, "cron"), aria_describedby: "schedule-cron-help schedule-cron-error",
                                oninput: move |event| { let mut next = form(); next.custom = event.value(); change.call(next); },
                            }
                            p { id: "schedule-cron-help", "minute hour day month weekday. Preview and save validate cron semantics on the server." }
                        } else {
                            TimePicker { id: "schedule-time", label: "Time · Bangkok (UTC+07:00)", required: true, value: current.time.clone(),
                                invalid: has_error(errors, "time"), describedby: "schedule-time-error schedule-cron-error",
                                on_input: move |value| { let mut next = form(); next.time = value; change.call(next); },
                            }
                        }
                        FieldMessage { errors: errors(), field: "cron" }
                        FieldMessage { errors: errors(), field: "time" }
                        label { class: "schedule-checkbox",
                            input { r#type: "checkbox", checked: current.enabled,
                                onchange: move |event| { let mut next = form(); next.enabled = event.checked(); change.call(next); },
                            }
                            "Enabled"
                        }
                    }
                    ErrorMessage { message: summary }
                    if recovery.read().reload_required && is_edit {
                        p { "The schedule changed. Your input is preserved; reload the current schedule to recover." }
                        button { r#type: "button", class: "ghost", disabled: busy,
                            onclick: move |_| confirm_reload.set(true), "Reload schedule" }
                    }
                    div { class: "schedule-actions",
                        button { r#type: "button", class: "ghost", disabled: busy || preview_state.read().pending() || current.preview().is_err(),
                            onclick: move |_| {
                                if saving() { return; }
                                let snapshot = form();
                                let query = match snapshot.preview() { Ok(query) => query, Err(validation) => { errors.set(validation); return; } };
                                let Some(ticket) = preview_state.write().begin() else { return; };
                                preview.set(None); recovery.write().edited(); errors.set(Vec::new());
                                spawn(async move {
                                    let result = session::with_session(|| api::cron_configurations::preview_configuration(query)).await;
                                    if !preview_state.write().finish(ticket) || form() != snapshot { return; }
                                    match result {
                                        Ok(value) => preview.set(Some(value)),
                                        Err(error) => {
                                            errors.set(vec![FieldError { field: backend_field(&error.to_string()), message: error.to_string() }]); recovery.write().transient_failed(error);
                                        }
                                    }
                                });
                            },
                            if preview_state.read().pending() { "Previewing…" } else { "Preview next five runs" }
                        }
                        button { r#type: "submit", class: "primary", disabled: busy || recovery.read().reload_required || current.create().is_err(),
                            if busy { "Saving…" } else { "Save schedule" }
                        }
                        button { r#type: "button", class: "ghost", disabled: busy,
                            onclick: move |_| { if form() == original() { on_close(()); } else { dismiss.set(true); } }, "Close editor" }
                    }
                }
                if let Some(value) = preview() {
                    section { class: "schedule-preview", aria_label: "Schedule preview", aria_live: "polite",
                        h4 { "{value.description}" }
                        p { "Next five executions · {value.schedule_timezone}. Periods include the start and exclude the end." }
                        ol {
                            for occurrence in &value.occurrences {
                                li {
                                    strong { "{bangkok_time(&occurrence.scheduled_for)}" }
                                    if let Some(period) = &occurrence.period {
                                        p { "[{bangkok_time(&period.period_start)}, {bangkok_time(&period.period_end_exclusive)})" }
                                    } else { p { "Inventory snapshot at execution time" } }
                                }
                            }
                        }
                    }
                }
            }
            if loading || load_error.is_some() {
                button { r#type: "button", class: "ghost", onclick: move |_| { if form() == original() { on_close(()); } else { dismiss.set(true); } }, "Close editor" }
            }
        }
        }
        super::navigation::DraftDepartureGuard { dirty }
        Confirm { open: dismiss(), title: "Discard unsaved changes?", message: "Your unsaved schedule changes will be lost.", confirm_label: "Discard changes", danger: true,
            on_cancel: move |()| dismiss.set(false), on_confirm: move |()| { dismiss.set(false); on_close(()); } }
        Confirm { open: confirm_reload(), title: "Reload schedule?", message: if dirty { "Reloading replaces your unsaved changes with the current saved schedule." } else { "Load the current saved schedule." }, confirm_label: "Reload schedule", danger: dirty,
            on_cancel: move |()| confirm_reload.set(false), on_confirm: move |()| { confirm_reload.set(false); reload += 1; } }
    }
}

#[component]
fn FieldMessage(errors: Vec<FieldError>, field: &'static str) -> Element {
    rsx! {
        div { id: "schedule-{field}-error", class: "schedule-field-error",
            for error in errors.iter().filter(|error| error.field == field) { p { "{error.message}" } }
        }
    }
}

fn has_error(errors: Signal<Vec<FieldError>>, field: &str) -> bool {
    errors.read().iter().any(|error| error.field == field)
}

fn cadence_value(cadence: Cadence) -> &'static str {
    match cadence {
        Cadence::PreviousMonth => "previous",
        Cadence::FirstHalf => "first-half",
        Cadence::Daily => "daily",
        Cadence::Weekly => "weekly",
        Cadence::Monthly => "monthly",
        Cadence::Custom => "custom",
    }
}

mod data;
mod detail;
mod editor;
mod files;
mod history;
mod navigation;
mod schedules;
mod state;
mod time;

use dioxus::prelude::*;
pub use navigation::use_schedule_history as use_scheduled_reports_history;
use state::use_schedule_owner;

#[component]
pub fn ScheduledReports(query: String) -> Element {
    let owner = use_schedule_owner();
    rsx! {
        main { class: "page scheduled-reports-page",
            h2 { "Scheduled reports" }
            nav { class: "schedule-tabs", aria_label: "Scheduled reports",
                Link { to: crate::route::Route::ScheduledReports { query: query.clone() }, aria_current: "page", "Schedules" }
                Link { to: "/scheduled-reports/history", "Report history" }
            }
            if let Some(owner) = owner() {
                schedules::Schedules { key: "{owner:?}", query }
            }
        }
    }
}

#[component]
pub fn ScheduledReportHistory(query: String) -> Element {
    let owner = use_schedule_owner();
    rsx! {
        main { class: "page scheduled-reports-page",
            h2 { "Scheduled reports" }
            nav { class: "schedule-tabs", aria_label: "Scheduled reports",
                Link { to: crate::route::Route::ScheduledReports { query: String::new() }, "Schedules" }
                Link { to: crate::route::Route::ScheduledReportHistory { query: query.clone() }, aria_current: "page", "Report history" }
            }
            if let Some(owner) = owner() {
                history::History { key: "{owner:?}:{query}", query }
            }
        }
    }
}

#[component]
pub fn ScheduledReportDetail(id: String, query: String) -> Element {
    let owner = use_schedule_owner();
    rsx! {
        main { class: "page scheduled-reports-page",
            h2 { "Saved report" }
            if let Some(owner) = owner() {
                detail::Detail { key: "{owner:?}:{id}:{query}", id, query }
            }
        }
    }
}

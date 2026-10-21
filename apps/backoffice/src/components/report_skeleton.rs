use dioxus::prelude::*;

#[component]
pub fn ReportSkeleton() -> Element {
    rsx! {
        div { role: "status", aria_live: "polite", aria_busy: "true",
            p { "Generating report…" }
            div { class: "report-skeleton", aria_hidden: "true",
                for _ in 0..5 { div { class: "report-skeleton-row" } }
            }
        }
    }
}

use super::csv::{HEADERS, cells};
use dioxus::prelude::*;
use domain::dto::ProductReportRow;

#[component]
pub fn ReportTable(rows: Vec<ProductReportRow>) -> Element {
    rsx! {
        div { class: "report-scroll", tabindex: "0", role: "region", aria_label: "Report products",
            table { class: "admins-table report-table",
                caption { class: "visually-hidden", "Report products" }
                thead { tr { for header in HEADERS { th { scope: "col", "{header}" } } } }
                tbody { for row in &rows { tr { for value in cells(row) { td { "{value}" } } } } }
            }
        }
    }
}

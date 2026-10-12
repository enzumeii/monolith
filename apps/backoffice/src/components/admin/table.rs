//! The admins table plus pagination.

use dioxus::prelude::*;
use domain::dto::AdminDTO;

use super::row::AdminRow;

#[component]
pub fn AdminTable(
    admins: Vec<AdminDTO>,
    has_next: bool,
    page: u32,
    my_id: i64,
    on_prev: EventHandler<()>,
    on_next: EventHandler<()>,
    on_changed: EventHandler<()>,
) -> Element {
    rsx! {
        table { class: "admins-table",
            thead {
                tr {
                    th { "ID" }
                    th { "Username" }
                    th { "Role" }
                    th { "Actions" }
                }
            }
            tbody {
                for admin in &admins {
                    AdminRow {
                        key: "{admin.id}",
                        admin: admin.clone(),
                        my_id,
                        on_changed,
                    }
                }
            }
        }
        div { class: "pager",
            button {
                class: "ghost",
                disabled: page == 1,
                onclick: move |_| on_prev(()),
                "Prev"
            }
            span { "Page {page}" }
            button {
                class: "ghost",
                disabled: !has_next,
                onclick: move |_| on_next(()),
                "Next"
            }
        }
    }
}

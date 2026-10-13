//! Admin management page (super admin only): fetch + orchestration.

mod create_form;
mod row;
mod table;

use self::create_form::AdminCreateForm;
use self::table::AdminTable;
use dioxus::prelude::*;
use domain::AdminRole;
use domain::dto::{AdminListResponse, ListAdminsQuery};

use crate::api;
use crate::components::ErrorMessage;
use crate::session::{self, SESSION};

const PER_PAGE: u32 = 25;

#[component]
pub fn Admins() -> Element {
    let mut list = use_signal(|| None::<AdminListResponse>);
    let mut error = use_signal(|| None::<String>);
    let mut page = use_signal(|| 1u32);
    let mut reload = use_signal(|| 0u32);
    let mut show_create = use_signal(|| false);

    use_effect(move || {
        let p = page();
        let _ = reload();
        spawn(async move {
            let query = ListAdminsQuery {
                page: Some(p),
                per_page: Some(PER_PAGE),
            };
            match session::with_session(|| api::list_admins(query)).await {
                Ok(resp) => {
                    list.set(Some(resp));
                    error.set(None);
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                }
            }
        });
    });

    let is_super = matches!(
        SESSION.read().as_ref().map(|s| s.role),
        Some(AdminRole::SuperAdmin)
    );
    let my_id = SESSION
        .read()
        .as_ref()
        .map(|s| s.admin_id)
        .unwrap_or_default();
    let list_data = list.read().clone();

    rsx! {
        main { class: "page",
            h2 { "Admin management" }
            if !is_super {
                ErrorMessage {
                    message: Some("Not authorized. Admin management requires a super admin.".into()),
                }
            } else {
                ErrorMessage { message: error() }
                div { class: "toolbar",
                    button {
                        class: "primary",
                        onclick: move |_| show_create.set(!show_create()),
                        if show_create() { "Close" } else { "Create Admin" }
                    }
                }
                if show_create() {
                    AdminCreateForm {
                        on_created: move |()| {
                            show_create.set(false);
                            *reload.write() += 1;
                        },
                    }
                }
                if let Some(data) = &list_data {
                    AdminTable {
                        admins: data.admins.clone(),
                        has_next: data.has_next,
                        page: page(),
                        my_id,
                        on_prev: move |()| {
                            page.set(page().saturating_sub(1).max(1));
                        },
                        on_next: move |()| {
                            page.set(page().saturating_add(1));
                        },
                        on_changed: move |()| {
                            *reload.write() += 1;
                        },
                    }
                } else if error().is_none() {
                    p { "Loading admins…" }
                }
            }
        }
    }
}

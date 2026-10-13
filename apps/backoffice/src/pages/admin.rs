//! Admin management page (super admin only): fetch + orchestration.

mod create_form;
mod row;
mod table;

use self::create_form::AdminCreateForm;
use self::table::AdminTable;
use dioxus::prelude::*;
use domain::AdminRole;
use domain::dto::ListAdminsQuery;

use crate::api;
use crate::components::ErrorMessage;
use crate::session::{self, SESSION};

const PER_PAGE: u32 = 25;

#[component]
pub fn Admins() -> Element {
    let mut page = use_signal(|| 1u32);
    let mut show_create = use_signal(|| false);

    let mut admins = use_resource(move || async move {
        let p = page();
        let is_super = SESSION
            .read()
            .as_ref()
            .is_some_and(|session| session.role == AdminRole::SuperAdmin);
        if !is_super {
            return Err(api::ApiError::new(
                reqwest::StatusCode::FORBIDDEN,
                "Not authorized",
            ));
        }
        session::with_session(|| {
            api::list_admins(ListAdminsQuery {
                page: Some(p),
                per_page: Some(PER_PAGE),
            })
        })
        .await
    });
    use_effect(move || {
        if *admins.state().read() == UseResourceState::Ready
            && let Some(Ok(data)) = admins.read().as_ref()
            && data.page > 1
            && data.admins.is_empty()
        {
            page.set(data.page - 1);
        }
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
    let loading = *admins.state().read() == UseResourceState::Pending;
    let result = admins.read().clone();
    let list_data = result
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .cloned();
    let error = if loading {
        None
    } else {
        result.and_then(Result::err).map(|error| error.to_string())
    };

    rsx! {
        main { class: "page",
            h2 { "Admin management" }
            if !is_super {
                ErrorMessage {
                    message: Some("Not authorized. Admin management requires a super admin.".into()),
                }
            } else {
                ErrorMessage { message: error.clone() }
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
                            admins.restart();
                        },
                    }
                }
                if loading && list_data.is_some() {p {role:"status", "Loading admins…"}}
                if let Some(data) = &list_data {
                    AdminTable {
                        admins: data.admins.clone(),
                        has_next: data.has_next,
                        page: data.page,
                        loading,
                        my_id,
                        on_prev: move |()| {
                            page.set(page().saturating_sub(1).max(1));
                        },
                        on_next: move |()| {
                            page.set(page().saturating_add(1));
                        },
                        on_changed: move |()| {
                            admins.restart();
                        },
                    }
                } else if error.is_none() {
                    p { "Loading admins…" }
                }
            }
        }
    }
}

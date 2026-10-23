//! One admin row with inline edit + delete (both behind a confirm dialog).

use dioxus::prelude::*;
use domain::dto::{AdminDTO, UpdateAdminRequest};

use crate::api;
use crate::components::Confirm;
use crate::session;

#[component]
pub fn AdminRow(admin: AdminDTO, my_id: i64, on_changed: EventHandler<()>) -> Element {
    let mut editing = use_signal(|| false);
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut confirm_save = use_signal(|| false);
    let mut confirm_delete = use_signal(|| false);

    let id = admin.id;
    let role = admin.role;
    let name = admin.username.clone();
    let name_edit = admin.username.clone();
    let is_self = id == my_id;

    let save = move |()| {
        let u = username.read().clone();
        let p = password.read().clone();
        confirm_save.set(false);
        if u.trim().is_empty() {
            return;
        }
        spawn(async move {
            let req = UpdateAdminRequest {
                username: Some(u),
                password: (!p.is_empty()).then_some(p),
            };
            let result = if is_self {
                session::update_self(id, req).await
            } else {
                session::with_session(|| api::update_admin(id, req)).await
            };
            match result {
                Ok(_) => {
                    if !is_self {
                        editing.set(false);
                        error.set(None);
                        on_changed(());
                    }
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                }
            }
        });
    };

    rsx! {
        tr {
            td { "{id}" }
            td {
                if editing() {
                    input {
                        r#type: "text",
                        value: "{username}",
                        oninput: move |e| username.set(e.value()),
                    }
                } else {
                    "{name}"
                }
            }
            td { "{role}" }
            td { class: "row-actions",
                if editing() {
                    input {
                        r#type: "password",
                        placeholder: "new password (optional)",
                        autocomplete: "new-password",
                        value: "{password}",
                        oninput: move |e| password.set(e.value()),
                    }
                    button {
                        class: "primary",
                        disabled: username.read().trim().is_empty(),
                        onclick: move |_| { if !username.read().trim().is_empty() { confirm_save.set(true); } },
                        "Save"
                    }
                    button {
                        class: "ghost",
                        onclick: move |_| editing.set(false),
                        "Cancel"
                    }
                } else {
                    button {
                        class: "ghost",
                        onclick: move |_| {
                            username.set(name_edit.clone());
                            password.set(String::new());
                            editing.set(true);
                        },
                        "Edit"
                    }
                    button {
                        class: "danger",
                        disabled: is_self,
                        onclick: move |_| confirm_delete.set(true),
                        "Delete"
                    }
                }
            }
        }
        if let Some(msg) = error() {
            tr {
                td { colspan: "4", class: "error", role: "alert", "{msg}" }
            }
        }
        Confirm {
            open: confirm_save(),
            title: "Update admin",
            message: "Save changes to {name}?",
            confirm_label: "Save",
            danger: false,
            on_confirm: save,
            on_cancel: move |()| confirm_save.set(false),
        }
        Confirm {
            open: confirm_delete(),
            title: "Delete admin",
            message: "Permanently delete {name}? This cannot be undone.",
            confirm_label: "Delete",
            danger: true,
            on_confirm: move |()| {
                confirm_delete.set(false);
                spawn(async move {
                    match session::with_session(|| api::delete_admin(id)).await {
                        Ok(_) => {
                            error.set(None);
                            on_changed(());
                        }
                        Err(e) => {
                            error.set(Some(e.to_string()));
                        }
                    }
                });
            },
            on_cancel: move |()| confirm_delete.set(false),
        }
    }
}

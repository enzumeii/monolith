//! One admin row with inline edit + delete.

use dioxus::prelude::*;
use domain::dto::{AdminDTO, UpdateAdminRequest};

use crate::api;
use crate::session;

#[component]
pub fn AdminRow(admin: AdminDTO, my_id: i64, on_changed: EventHandler<()>) -> Element {
    let mut editing = use_signal(|| false);
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);

    let id = admin.id;
    let role = admin.role;
    let name = admin.username.clone();
    let name_edit = admin.username.clone();
    let is_self = id == my_id;

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
                        onclick: move |_| {
                            let u = username.read().clone();
                            let p = password.read().clone();
                            spawn(async move {
                                let req = UpdateAdminRequest {
                                    username: Some(u),
                                    password: (!p.is_empty()).then_some(p),
                                };
                                match session::with_session(|t| api::update_admin(id, req, t)).await {
                                    Ok(_) => {
                                        if is_self {
                                            session::sign_out();
                                        } else {
                                            editing.set(false);
                                            error.set(None);
                                            on_changed(());
                                        }
                                    }
                                    Err(e) => {
                                        session::handle_api_error(&e);
                                        error.set(Some(e.to_string()));
                                    }
                                }
                            });
                        },
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
                        onclick: move |_| {
                            spawn(async move {
                                match session::with_session(|t| api::delete_admin(id, t)).await {
                                    Ok(_) => {
                                        error.set(None);
                                        on_changed(());
                                    }
                                    Err(e) => {
                                        session::handle_api_error(&e);
                                        error.set(Some(e.to_string()));
                                    }
                                }
                            });
                        },
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
    }
}

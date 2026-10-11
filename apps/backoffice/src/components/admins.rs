//! Admin management page (super admin only).
//!
//! Lists all admins with pagination, and provides a create-admin form plus
//! per-row edit and delete. The tab is hidden from regular admins in the
//! navbar; this page also guards the route so a normal admin who navigates
//! directly is told they are not authorized.

use dioxus::prelude::*;
use domain::AdminRole;
use domain::dto::{AdminDTO, AdminListResponse};

use crate::api;
use crate::session::{self, SESSION};

const PER_PAGE: u32 = 25;

#[component]
pub fn Admins() -> Element {
    let mut list = use_signal(|| None::<AdminListResponse>);
    let mut error = use_signal(|| None::<String>);
    let mut page = use_signal(|| 1u32);
    let mut reload = use_signal(|| 0u32);

    // Create-admin form state.
    let mut show_create = use_signal(|| false);
    let mut c_user = use_signal(String::new);
    let mut c_pass = use_signal(String::new);
    let mut c_role = use_signal(AdminRole::default);

    // Fetch the list whenever the page or a mutation bumps `reload`.
    use_effect(move || {
        let p = page();
        let _ = reload();
        spawn(async move {
            let Some(s) = session::current() else {
                return;
            };
            match api::list_admins(s, p, PER_PAGE).await {
                Ok((ns, resp)) => {
                    session::adopt(ns);
                    list.set(Some(resp));
                    error.set(None);
                }
                Err(e) => {
                    session::handle_api_error(&e);
                    error.set(Some(e.to_string()));
                }
            }
        });
    });

    let session = SESSION.read().clone();
    let is_super = matches!(
        session.as_ref().map(|s| s.role),
        Some(AdminRole::SuperAdmin)
    );
    let my_id = session.as_ref().map(|s| s.admin_id).unwrap_or_default();
    let list_data = list.read().clone();

    rsx! {
        main { class: "page",
            h2 { "Admin management" }
            if !is_super {
                p { class: "error", role: "alert",
                    "Not authorized. Admin management requires a super admin."
                }
            } else {
                if let Some(msg) = error() {
                    p { class: "error", role: "alert", "{msg}" }
                }

                div { class: "toolbar",
                    button {
                        class: "primary",
                        onclick: move |_| {
                            let cur = show_create();
                            show_create.set(!cur);
                        },
                        if show_create() { "Close" } else { "Create Admin" }
                    }
                }

                if show_create() {
                    form {
                        class: "panel create-form",
                        onsubmit: move |e| {
                            e.prevent_default();
                            let u = c_user.read().clone();
                            let p = c_pass.read().clone();
                            let r = c_role();
                            if u.is_empty() || p.is_empty() {
                                error.set(Some("username and password are required".into()));
                                return;
                            }
                            error.set(None);
                            spawn(async move {
                                let Some(s) = session::current() else {
                                    return;
                                };
                                match api::create_admin(s, u, p, r).await {
                                    Ok((ns, _)) => {
                                        session::adopt(ns);
                                        show_create.set(false);
                                        c_user.set(String::new());
                                        c_pass.set(String::new());
                                        *reload.write() += 1;
                                    }
                                    Err(e) => {
                                        session::handle_api_error(&e);
                                        error.set(Some(e.to_string()));
                                    }
                                }
                            });
                        },
                        h3 { "Create admin" }
                        label { r#for: "create-username", "Username" }
                        input {
                            id: "create-username",
                            r#type: "text",
                            autocomplete: "off",
                            value: "{c_user}",
                            oninput: move |e| c_user.set(e.value()),
                        }
                        label { r#for: "create-password", "Password" }
                        input {
                            id: "create-password",
                            r#type: "password",
                            autocomplete: "new-password",
                            value: "{c_pass}",
                            oninput: move |e| c_pass.set(e.value()),
                        }
                        label { r#for: "create-role", "Role" }
                        select {
                            id: "create-role",
                            value: "{c_role}",
                            onchange: move |e| {
                                c_role.set(if e.value() == "SUPER_ADMIN" {
                                    AdminRole::SuperAdmin
                                } else {
                                    AdminRole::Admin
                                });
                            },
                            option { value: "ADMIN", "Admin" }
                            option { value: "SUPER_ADMIN", "Super Admin" }
                        }
                        button { r#type: "submit", class: "primary", "Create" }
                    }
                }

                if let Some(data) = &list_data {
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
                            for admin in &data.admins {
                                AdminRow {
                                    key: "{admin.id}",
                                    admin: admin.clone(),
                                    my_id: my_id,
                                    on_changed: move |()| {
                                        *reload.write() += 1;
                                    },
                                }
                            }
                        }
                    }
                    div { class: "pager",
                        button {
                            class: "ghost",
                            disabled: page() == 1,
                            onclick: move |_| {
                                if page() > 1 {
                                    *page.write() -= 1;
                                }
                            },
                            "Prev"
                        }
                        span { "Page {page}" }
                        button {
                            class: "ghost",
                            disabled: !data.has_next,
                            onclick: move |_| {
                                *page.write() += 1;
                            },
                            "Next"
                        }
                    }
                } else if error().is_none() {
                    p { "Loading admins…" }
                }
            }
        }
    }
}

/// One row of the admin table, with inline edit + delete.
#[component]
fn AdminRow(admin: AdminDTO, my_id: i64, on_changed: EventHandler<()>) -> Element {
    let mut editing = use_signal(|| false);
    let mut e_user = use_signal(String::new);
    let mut e_pass = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);

    let id = admin.id;
    let role = admin.role;
    let uname = admin.username.clone();
    let uname_edit = admin.username.clone();
    let is_self = id == my_id;

    rsx! {
        tr {
            td { "{id}" }
            td {
                if editing() {
                    input {
                        r#type: "text",
                        value: "{e_user}",
                        oninput: move |e| e_user.set(e.value()),
                    }
                } else {
                    "{uname}"
                }
            }
            td { "{role}" }
            td { class: "row-actions",
                if editing() {
                    input {
                        r#type: "password",
                        placeholder: "new password (optional)",
                        autocomplete: "new-password",
                        value: "{e_pass}",
                        oninput: move |e| e_pass.set(e.value()),
                    }
                    button {
                        class: "primary",
                        onclick: move |_| {
                            let u = e_user.read().clone();
                            let p = e_pass.read().clone();
                            let p_opt = if p.is_empty() { None } else { Some(p) };
                            spawn(async move {
                                let Some(s) = session::current() else {
                                    return;
                                };
                                match api::update_admin(s, id, Some(u), p_opt).await {
                                    Ok((ns, _)) => {
                                        session::adopt(ns);
                                        editing.set(false);
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
                            e_user.set(uname_edit.clone());
                            e_pass.set(String::new());
                            editing.set(true);
                        },
                        "Edit"
                    }
                    button {
                        class: "danger",
                        disabled: is_self,
                        onclick: move |_| {
                            spawn(async move {
                                let Some(s) = session::current() else {
                                    return;
                                };
                                match api::delete_admin(s, id).await {
                                    Ok((ns, _)) => {
                                        session::adopt(ns);
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

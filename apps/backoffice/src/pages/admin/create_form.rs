//! Create-admin form (behind a confirm dialog).

use dioxus::prelude::*;
use domain::AdminRole;
use domain::dto::CreateAdminRequest;

use crate::api;
use crate::components::Confirm;
use crate::components::{ErrorMessage, FormField};
use crate::session;

#[component]
pub fn AdminCreateForm(on_created: EventHandler<()>) -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut role = use_signal(AdminRole::default);
    let mut error = use_signal(|| None::<String>);
    let mut confirm = use_signal(|| false);

    rsx! {
        form {
            class: "panel create-form",
            onsubmit: move |e| {
                e.prevent_default();
                if username.read().is_empty() || password.read().is_empty() {
                    error.set(Some("username and password are required".into()));
                    return;
                }
                error.set(None);
                confirm.set(true);
            },
            h3 { "Create admin" }
            FormField {
                label: "Username",
                id: "create-username",
                input_type: "text",
                autocomplete: "off",
                value: username(),
                on_input: move |v: String| username.set(v),
            }
            FormField {
                label: "Password",
                id: "create-password",
                input_type: "password",
                autocomplete: "new-password",
                value: password(),
                on_input: move |v: String| password.set(v),
            }
            label { r#for: "create-role", "Role" }
            select {
                id: "create-role",
                value: "{role}",
                onchange: move |e| {
                    role.set(if e.value() == "SUPER_ADMIN" {
                        AdminRole::SuperAdmin
                    } else {
                        AdminRole::Admin
                    });
                },
                option { value: "ADMIN", "Admin" }
                option { value: "SUPER_ADMIN", "Super Admin" }
            }
            ErrorMessage { message: error() }
            button { r#type: "submit", class: "primary", "Create" }
        }
        Confirm {
            open: confirm(),
            title: "Create admin",
            message: format!("Create admin \"{}\"?", username.read()),
            confirm_label: "Create",
            danger: false,
            on_confirm: move |()| {
                confirm.set(false);
                let req = CreateAdminRequest {
                    username: username.read().clone(),
                    password: password.read().clone(),
                    role: role(),
                };
                spawn(async move {
                    match session::with_session(|| api::create_admin(req)).await {
                        Ok(_) => {
                            username.set(String::new());
                            password.set(String::new());
                            on_created(());
                        }
                        Err(e) => {

                            error.set(Some(e.to_string()));
                        }
                    }
                });
            },
            on_cancel: move |()| confirm.set(false),
        }
    }
}

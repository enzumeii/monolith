//! Create-admin form.

use dioxus::prelude::*;
use domain::AdminRole;
use domain::dto::CreateAdminRequest;

use crate::api;
use crate::components::form::{ErrorMessage, FormField};
use crate::session;

#[component]
pub fn AdminCreateForm(on_created: EventHandler<()>) -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut role = use_signal(AdminRole::default);
    let mut error = use_signal(|| None::<String>);

    rsx! {
        form {
            class: "panel create-form",
            onsubmit: move |e| {
                e.prevent_default();
                let u = username.read().clone();
                let p = password.read().clone();
                let r = role();
                if u.is_empty() || p.is_empty() {
                    error.set(Some("username and password are required".into()));
                    return;
                }
                error.set(None);
                spawn(async move {
                    let req = CreateAdminRequest {
                        username: u,
                        password: p,
                        role: r,
                    };
                    match session::with_session(|t| api::create_admin(req, t)).await {
                        Ok(_) => {
                            username.set(String::new());
                            password.set(String::new());
                            on_created(());
                        }
                        Err(e) => {
                            session::handle_api_error(&e);
                            error.set(Some(e.to_string()));
                        }
                    }
                });
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
    }
}

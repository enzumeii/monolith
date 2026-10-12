//! Login screen (`/v1/auth/login` via [`crate::session::sign_in`]).

use dioxus::prelude::*;

use crate::components::form::{ErrorMessage, FormField};
use crate::session;

#[component]
pub fn Login() -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    rsx! {
        div { class: "login-page",
            form {
                class: "login-card",
                onsubmit: move |e| {
                    e.prevent_default();
                    if busy() {
                        return;
                    }
                    busy.set(true);
                    error.set(None);
                    let u = username.read().clone();
                    let p = password.read().clone();
                    spawn(async move {
                        match session::sign_in(u, p).await {
                            Ok(()) => {}
                            Err(e) => error.set(Some(e.to_string())),
                        }
                        busy.set(false);
                    });
                },
                h1 { class: "login-title", "Backoffice" }
                FormField {
                    label: "Username",
                    id: "login-username",
                    input_type: "text",
                    autocomplete: "username",
                    value: username(),
                    on_input: move |v: String| username.set(v),
                }
                FormField {
                    label: "Password",
                    id: "login-password",
                    input_type: "password",
                    autocomplete: "current-password",
                    value: password(),
                    on_input: move |v: String| password.set(v),
                }
                ErrorMessage { message: error() }
                button { r#type: "submit", class: "primary", disabled: busy(),
                    if busy() { "Signing in…" } else { "Sign in" }
                }
            }
        }
    }
}

//! Login screen (`/v1/auth/login` via the `api::login` server function).

use dioxus::prelude::*;

use crate::api;
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
                        match api::login(u, p).await {
                            Ok(s) => session::sign_in(s),
                            Err(e) => error.set(Some(e.to_string())),
                        }
                        busy.set(false);
                    });
                },
                h1 { class: "login-title", "Backoffice" }
                label { r#for: "login-username", "Username" }
                input {
                    id: "login-username",
                    r#type: "text",
                    autocomplete: "username",
                    value: "{username}",
                    oninput: move |e| username.set(e.value()),
                }
                label { r#for: "login-password", "Password" }
                input {
                    id: "login-password",
                    r#type: "password",
                    autocomplete: "current-password",
                    value: "{password}",
                    oninput: move |e| password.set(e.value()),
                }
                if let Some(msg) = error() {
                    p { class: "error", role: "alert", "{msg}" }
                }
                button { r#type: "submit", class: "primary", disabled: busy(),
                    if busy() { "Signing in…" } else { "Sign in" }
                }
            }
        }
    }
}

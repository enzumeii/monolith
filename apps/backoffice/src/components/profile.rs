//! Profile page: any admin can update their own username and/or password.
//!
//! Updating your own account revokes your refresh tokens server-side, so a
//! successful save signs you out and returns you to the login screen.

use dioxus::prelude::*;

use crate::api;
use crate::session::{self, SESSION};

#[component]
pub fn Profile() -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

    // Pre-fill the username from the session once mounted.
    use_effect(move || {
        if let Some(s) = SESSION.read().clone()
            && username.peek().is_empty()
        {
            username.set(s.username.clone());
        }
    });

    let session = SESSION.read().clone();
    let my_id = session.as_ref().map(|s| s.admin_id).unwrap_or_default();

    rsx! {
        main { class: "page",
            if let Some(s) = &session {
                h2 { "Profile" }
                p { class: "meta", "Signed in as {s.username} ({s.role}), id {s.admin_id}." }
                form {
                    class: "panel",
                    onsubmit: move |e| {
                        e.prevent_default();
                        if busy() {
                            return;
                        }
                        let u = username.read().clone();
                        let p = password.read().clone();
                        let c = confirm.read().clone();
                        if u.is_empty() {
                            error.set(Some("username cannot be empty".into()));
                            return;
                        }
                        if !p.is_empty() && p != c {
                            error.set(Some("passwords do not match".into()));
                            return;
                        }
                        busy.set(true);
                        error.set(None);
                        let id = my_id;
                        spawn(async move {
                            let Some(cur) = session::current() else {
                                return;
                            };
                            let p_opt = if p.is_empty() { None } else { Some(p) };
                            match api::update_admin(cur, id, Some(u), p_opt).await {
                                // Self-update revokes our tokens; require re-login.
                                Ok(_) => session::sign_out(),
                                Err(e) => {
                                    session::handle_api_error(&e);
                                    error.set(Some(e.to_string()));
                                    busy.set(false);
                                }
                            }
                        });
                    },
                    label { r#for: "profile-username", "Username" }
                    input {
                        id: "profile-username",
                        r#type: "text",
                        autocomplete: "username",
                        value: "{username}",
                        oninput: move |e| username.set(e.value()),
                    }
                    label { r#for: "profile-password", "New password" }
                    input {
                        id: "profile-password",
                        r#type: "password",
                        autocomplete: "new-password",
                        value: "{password}",
                        oninput: move |e| password.set(e.value()),
                    }
                    label { r#for: "profile-confirm", "Confirm new password" }
                    input {
                        id: "profile-confirm",
                        r#type: "password",
                        autocomplete: "new-password",
                        value: "{confirm}",
                        oninput: move |e| confirm.set(e.value()),
                    }
                    if let Some(msg) = error() {
                        p { class: "error", role: "alert", "{msg}" }
                    }
                    p { class: "hint", "Saving updates your account and signs you out." }
                    button { r#type: "submit", class: "primary", disabled: busy(),
                        if busy() { "Saving…" } else { "Save changes" }
                    }
                }
            } else {
                p { "Not signed in." }
            }
        }
    }
}

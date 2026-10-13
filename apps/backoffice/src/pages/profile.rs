//! Profile page: any admin can update their own username and/or password.
//! Saving signs you out (the API revokes your tokens), so you log in again.

use dioxus::prelude::*;
use domain::dto::UpdateAdminRequest;

use crate::components::Confirm;
use crate::components::{ErrorMessage, FormField};
use crate::session::{self, SESSION};

#[component]
pub fn Profile() -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut confirm_save = use_signal(|| false);

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
                        error.set(None);
                        confirm_save.set(true);
                    },
                    FormField {
                        label: "Username",
                        id: "profile-username",
                        input_type: "text",
                        autocomplete: "username",
                        value: username(),
                        on_input: move |v: String| username.set(v),
                    }
                    FormField {
                        label: "New password",
                        id: "profile-password",
                        input_type: "password",
                        autocomplete: "new-password",
                        value: password(),
                        on_input: move |v: String| password.set(v),
                    }
                    FormField {
                        label: "Confirm new password",
                        id: "profile-confirm",
                        input_type: "password",
                        autocomplete: "new-password",
                        value: confirm(),
                        on_input: move |v: String| confirm.set(v),
                    }
                    ErrorMessage { message: error() }
                    p { class: "hint", "Saving updates your account and signs you out." }
                    button { r#type: "submit", class: "primary", disabled: busy(),
                        if busy() { "Saving…" } else { "Save changes" }
                    }
                }
            } else {
                p { "Not signed in." }
            }
        }
        Confirm {
            open: confirm_save(),
            title: "Update profile",
            message: "Save your changes? You will be signed out.",
            confirm_label: "Save",
            danger: false,
            on_confirm: move |()| {
                confirm_save.set(false);
                busy.set(true);
                let id = my_id;
                let req = UpdateAdminRequest {
                    username: Some(username.read().clone()),
                    password: (!password.read().is_empty()).then(|| password.read().clone()),
                };
                spawn(async move {
                    match session::update_self(id,req).await {
                        Ok(_) => {},
                        Err(e) => {

                            error.set(Some(e.to_string()));
                            busy.set(false);
                        }
                    }
                });
            },
            on_cancel: move |()| confirm_save.set(false),
        }
    }
}

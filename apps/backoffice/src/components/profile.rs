//! Profile page: any admin can update their own username and/or password.
//! Saving signs you out (the API revokes your tokens), so you log in again.

use dioxus::prelude::*;
use domain::dto::UpdateAdminRequest;

use crate::api;
use crate::components::form::{ErrorMessage, FormField};
use crate::session::{self, SESSION};

#[component]
pub fn Profile() -> Element {
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut confirm = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);

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
                            let req = UpdateAdminRequest {
                                username: Some(u),
                                password: (!p.is_empty()).then_some(p),
                            };
                            match session::with_session(|t| api::update_admin(id, req, t)).await {
                                Ok(_) => session::sign_out(),
                                Err(e) => {
                                    session::handle_api_error(&e);
                                    error.set(Some(e.to_string()));
                                    busy.set(false);
                                }
                            }
                        });
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
    }
}

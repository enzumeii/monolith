use crate::{
    api,
    components::{
        Confirm, ErrorMessage,
        request_state::{RequestState, use_owner},
    },
    session,
};
use dioxus::prelude::*;

#[component]
pub(super) fn DeleteConvention(
    id: String,
    name: String,
    mut request: Signal<RequestState>,
    on_deleted: EventHandler<()>,
    #[props(default)] disabled: bool,
    #[props(default)] quiet: bool,
) -> Element {
    let owner = use_owner();
    let mut confirmation = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    use_effect(move || {
        let _ = owner();
        confirmation.set(false);
        error.set(None);
    });
    let confirm_id = id.clone();
    rsx! {
        div { class: "convention-delete-action",
            button { id: "delete-convention-{id}", r#type: "button", class: if quiet { "ghost" } else { "danger" }, aria_label: "Delete {name}", disabled: disabled || request.read().pending() || owner().is_none(), onclick: move |_| { error.set(None); confirmation.set(true); }, if request.read().pending() { "Deleting…" } else { "Delete" } }
            ErrorMessage { message: error() }
        }
        Confirm {
            open: confirmation(), title: "Delete ‘{name}’?",
            message: "This permanently deletes the convention and its selected variants, including unsaved changes. Uploaded logo files remain in storage.",
            confirm_label: "Delete convention", danger: true,
            on_cancel: move |()| confirmation.set(false),
            on_confirm: move |()| {
                if disabled || !confirmation() { return; }
                let Some(ticket) = request.write().begin() else { return; };
                confirmation.set(false); error.set(None);
                let id = confirm_id.clone(); let request_owner = owner();
                spawn(async move {
                    let result = session::with_session(|| api::conventions::delete(&id)).await;
                    if owner() != request_owner || !request.write().finish(ticket) { return; }
                    match result { Ok(()) => on_deleted(()), Err(value) => error.set(Some(value.to_string())) }
                });
            }
        }
    }
}

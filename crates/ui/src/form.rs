use dioxus::prelude::*;

#[component]
pub fn FormField(
    label: &'static str,
    id: &'static str,
    input_type: &'static str,
    autocomplete: &'static str,
    value: String,
    on_input: EventHandler<String>,
) -> Element {
    rsx! {
        label { r#for: id, "{label}" }
        input {
            id,
            r#type: input_type,
            autocomplete,
            value: "{value}",
            oninput: move |e| on_input(e.value()),
        }
    }
}

#[component]
pub fn ErrorMessage(message: Option<String>) -> Element {
    rsx! {
        if let Some(msg) = message {
            p { class: "error", role: "alert", "{msg}" }
        }
    }
}

#[component]
pub fn LoadingMessage(message: String) -> Element {
    rsx! { p { role: "status", "{message}" } }
}

#[cfg(test)]
#[path = "tests/form.rs"]
mod tests;

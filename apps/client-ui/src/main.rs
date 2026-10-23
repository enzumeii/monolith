use dioxus::prelude::*;
use ui::{COMMON_STYLESHEET, ErrorMessage, LoadingMessage};

mod config;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut settings = use_resource(config::Settings::new);
    rsx! {
        document::Stylesheet { href: COMMON_STYLESHEET }
        Configuration {
            state: settings.read().clone(),
            retry: move |()| { settings.restart(); },
        }
    }
}

#[component]
fn Configuration(state: Option<Result<config::Settings, String>>, retry: EventHandler) -> Element {
    match state {
        Some(Ok(settings)) => rsx! { Application { settings } },
        Some(Err(message)) => rsx! {
            ErrorMessage { message: Some(message) }
            button { onclick: move |_| retry.call(()), "Retry" }
        },
        None => rsx! { LoadingMessage { message: "Loading configuration…" } },
    }
}

#[component]
fn Application(settings: config::Settings) -> Element {
    use_context_provider(move || settings);
    rsx! {
        main {
            h1 { "Enzumeii" }
            p { "Client application" }
        }
    }
}

#[cfg(test)]
#[path = "tests/app.rs"]
mod tests;

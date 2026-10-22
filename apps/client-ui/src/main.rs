use dioxus::prelude::*;

mod config;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut settings = use_resource(config::Settings::new);
    rsx! {
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
            p { role: "alert", "{message}" }
            button { onclick: move |_| retry.call(()), "Retry" }
        },
        None => rsx! { p { "Loading configuration…" } },
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

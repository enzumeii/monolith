use dioxus::prelude::*;
use ui::{COMMON_STYLESHEET, ErrorMessage, LoadingMessage, SITE_LOGO};

mod api;
mod cart;
mod checkout;
mod config;
mod country;
mod pages;
mod route;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", r#type: "image/png", href: SITE_LOGO }
        document::Stylesheet { href: COMMON_STYLESHEET }
        document::Stylesheet { href: asset!("/assets/portal.css") }
        ui::LoadingBoundary { Startup {} }
    }
}

#[component]
fn Startup() -> Element {
    let settings = use_resource(config::Settings::new);
    rsx! { Configuration { state: settings.read().clone() } }
}

#[component]
fn Configuration(state: Option<api::ApiResult<config::Settings>>) -> Element {
    match state {
        Some(Ok(settings)) => rsx! { Application { settings } },
        Some(Err(error)) => rsx! {
            ErrorMessage { message: Some(error.message) }
        },
        None => rsx! { LoadingMessage { message: "Loading configuration…" } },
    }
}

#[component]
fn Application(settings: config::Settings) -> Element {
    use_context_provider(move || settings);
    cart::use_cart_cache();
    rsx! {
        Router::<route::Route> {}
    }
}

#[cfg(test)]
#[path = "tests/app.rs"]
mod tests;

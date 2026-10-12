#![allow(rust_analyzer::inactive_code)]

mod api;
mod components;
mod config;
mod pages;
mod route;
mod server;
mod session;
mod state;

use dioxus::prelude::*;

use crate::pages::Login;
use crate::route::Route;
use crate::session::SESSION;

#[cfg(feature = "server")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = config::Settings::new()?;
    if std::env::args().any(|arg| arg == "--print-bind") {
        println!("{} {}", settings.server.host, settings.server.port);
        return Ok(());
    }
    server::run(state::AppState::new(settings), App)
}

#[cfg(not(feature = "server"))]
fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_effect(session::restore);

    rsx! {
        document::Stylesheet { href: asset!("/assets/main.css") }
        if SESSION.read().is_none() {
            Login {}
        } else {
            Router::<Route> {}
        }
    }
}

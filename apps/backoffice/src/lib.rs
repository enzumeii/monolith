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
pub fn run_server(print_bind: bool) -> Result<(), Box<dyn std::error::Error>> {
    let settings = config::Settings::new()?;
    if print_bind {
        println!("{} {}", settings.server.host, settings.server.port);
        return Ok(());
    }
    server::run(state::AppState::new(settings), App)
}

#[component]
pub fn App() -> Element {
    use_effect(|| {
        session::install_sync();
        spawn(async {
            let _ = session::restore().await;
        });
    });

    rsx! {
        document::Stylesheet { href: asset!("/assets/main.css") }
        if !*session::READY.read() {
            p { "Restoring session…" }
        } else if let Some(message) = session::RESTORE_ERROR.read().clone() {
            p { role: "alert", "{message}" }
            button { onclick: move |_| { spawn(async { let _=session::restore().await; }); }, "Retry" }
        } else if SESSION.read().is_none() {
            Login {}
        } else {
            Router::<Route> {}
        }
    }
}

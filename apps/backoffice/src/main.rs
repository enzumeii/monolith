//! Authed admin backoffice: login, admin management (super admin), profile.
//! Dioxus fullstack (web frontend + axum BFF server) that proxies to the
//! internal `services/api`; it never talks to the database directly.

mod api;
mod components;
mod config;
mod route;
mod session;

use dioxus::prelude::*;

use crate::components::Login;
use crate::route::Route;
use crate::session::SESSION;

fn main() {
    if std::env::args().any(|a| a == "--print-bind") {
        let s = config::settings();
        println!("{} {}", s.server.host, s.server.port);
        return;
    }
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Stylesheet { href: asset!("/assets/main.css") }
        if SESSION.read().is_none() {
            Login {}
        } else {
            Router::<Route> {}
        }
    }
}

//! Authed admin backoffice: login, admin management (super admin), profile.
//! Dioxus fullstack (web FE + axum server). The server proxies to the internal
//! `services/api` over HTTP using the base URL from `application.toml`
//! (overridable via env for Kubernetes). It never talks to the DB directly.

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
    // `dioxus::launch` auto-detects the platform from the enabled dioxus feature
    // (`web` for the wasm client, `server` for the fullstack axum server), so a
    // single entry point serves both builds. The server binds to the `IP`/`PORT`
    // env vars (set by `dx serve` for dev, or manually for deployment).
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

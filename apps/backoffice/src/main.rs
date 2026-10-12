mod api;
mod components;
mod config;
mod pages;
mod route;
mod session;

use dioxus::prelude::*;

use crate::pages::Login;
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
    // Restore any persisted session once on mount (after hydration; a no-op
    // during SSR, so server and initial client render match).
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

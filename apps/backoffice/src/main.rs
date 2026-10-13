use backoffice::{
    config,
    pages::Login,
    route::Route,
    session::{self, SESSION},
};
use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut settings = use_resource(config::Settings::new);
    rsx! {
        document::Stylesheet { href: asset!("/assets/main.css") }
        match settings.read().as_ref() {
            Some(Ok(settings)) => rsx! { Application { settings: settings.clone() } },
            Some(Err(message)) => rsx! {
                p { role: "alert", "{message}" }
                button { onclick: move |_| settings.restart(), "Retry" }
            },
            None => rsx! { p { "Loading configuration…" } },
        }
    }
}

#[component]
fn Application(settings: config::Settings) -> Element {
    use_context_provider(move || settings);
    use_effect(|| {
        session::install_sync();
        spawn(async {
            let _ = session::restore().await;
        });
    });

    rsx! {
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

use backoffice::{
    SITE_LOGO, config,
    pages::Login,
    route::Route,
    session::{self, SESSION},
    use_draft_history,
};
use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", r#type: "image/png", href: SITE_LOGO }
        document::Stylesheet { href: ui::COMMON_STYLESHEET }
        document::Stylesheet { href: asset!("/assets/main.css") }
        ui::LoadingBoundary { Startup {} }
    }
}

#[component]
fn Startup() -> Element {
    let mut settings = use_resource(|| async {
        let _loading = ui::loading_guard();
        config::Settings::new().await
    });
    rsx! {
        match settings.read().as_ref() {
            Some(Ok(settings)) => rsx! { Application { settings: settings.clone() } },
            Some(Err(message)) => rsx! {
                p { role: "alert", "{message}" }
                button { onclick: move |_| { settings.restart(); }, "Retry" }
            },
            None => rsx! { p { "Loading configuration…" } },
        }
    }
}

#[component]
fn Application(settings: config::Settings) -> Element {
    use_context_provider(move || settings);
    use_draft_history();
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
            button {
                onclick: move |_| {
                    spawn(async { let _ = session::restore().await; });
                },
                "Retry"
            }
        } else if SESSION.read().is_none() {
            Login {}
        } else {
            Router::<Route> {}
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "tests/app.rs"]
mod tests;

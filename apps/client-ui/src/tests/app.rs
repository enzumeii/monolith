use super::*;

fn render(state: Option<Result<config::Settings, String>>) -> String {
    fn app() -> Element {
        let state = use_context::<Option<Result<config::Settings, String>>>();
        rsx! { Configuration { state, retry: move |()| {} } }
    }
    let mut dom = VirtualDom::new(app).with_root_context(state);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[test]
fn configuration_loading_is_visible() {
    assert!(render(None).contains("Loading configuration"));
}

#[test]
fn configuration_failure_exposes_alert_and_retry() {
    let html = render(Some(Err("Unable to load application.toml".into())));
    assert!(html.contains("role=\"alert\""));
    assert!(html.contains("Unable to load application.toml"));
    assert!(html.contains(">Retry</button>"));
    assert!(!html.contains("Client application"));
}

#[test]
fn configured_shell_renders_without_inventing_customer_features() {
    let settings = toml::from_str(include_str!("../../application.local.toml")).expect("settings");
    let html = render(Some(Ok(settings)));
    assert!(html.contains("Enzumeii"));
    assert!(html.contains("Client application"));
    assert!(!html.contains("Retry"));
}

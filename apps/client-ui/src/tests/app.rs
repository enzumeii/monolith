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
    let html = render(None);
    assert!(html.contains("Loading configuration"));
    assert!(html.contains("role=\"status\""));
}

#[test]
fn configuration_failure_exposes_alert_and_retry() {
    let html = render(Some(Err("Unable to load application.toml".into())));
    assert!(html.contains("role=\"alert\""));
    assert!(html.contains("class=\"error\""));
    assert!(html.contains("Unable to load application.toml"));
    assert!(html.contains(">Retry</button>"));
    assert!(!html.contains("Client application"));
}

pub(crate) fn variant() -> domain::dto::PublicVariant {
    domain::dto::PublicVariant {
        id: "gid://shopify/ProductVariant/1".into(),
        title: "Blue / Large".into(),
        sku: Some("SHIRT-BL-L".into()),
        image: None,
        selected_options: vec![],
        final_price: "12.30".into(),
        inventory_policy: domain::dto::CatalogueInventoryPolicy::Deny,
        available_quantity: 10,
        max_quantity: 10,
    }
}

pub(crate) fn product() -> domain::dto::PublicProduct {
    domain::dto::PublicProduct {
        id: "gid://shopify/Product/1".into(),
        title: "Shirt".into(),
        description: "Soft cotton".into(),
        product_type: "Clothing".into(),
        featured_image: None,
        variants: vec![variant()],
    }
}

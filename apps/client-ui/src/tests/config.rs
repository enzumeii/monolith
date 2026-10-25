use super::resolve_config_url;

#[test]
fn config_asset_resolves_from_nested_routes() {
    assert_eq!(
        resolve_config_url(
            "/assets/application.toml",
            "https://admin.example.test/admins?page=2"
        )
        .expect("valid page and asset URL")
        .as_str(),
        "https://admin.example.test/assets/application.toml"
    );
}

#[test]
fn config_asset_preserves_hosted_base_path() {
    assert_eq!(
        resolve_config_url(
            "/user/assets/application.toml",
            "https://admin.example.test/user/profile"
        )
        .expect("valid page and asset URL")
        .as_str(),
        "https://admin.example.test/user/assets/application.toml"
    );
}

#[test]
fn config_asset_rejects_invalid_page_url() {
    assert!(resolve_config_url("/assets/application.toml", "not a URL").is_err());
}

#[test]
fn deployment_configs_select_public_api_without_changing_local_urls() {
    let local: super::Settings =
        toml::from_str(include_str!("../../application.toml")).expect("local settings");
    let production: super::Settings =
        toml::from_str(include_str!("../../application.prod.toml")).expect("production settings");
    assert_eq!(local.api.base_url, "http://localhost:3000/v1");
    assert_eq!(production.api.base_url, "/api/v1");
}

#[test]
fn relative_api_uses_the_current_generated_origin() {
    assert_eq!(
        super::resolve_api_url("/api/v1", "https://preview-123.vercel.app/user/orders")
            .expect("relative API"),
        "https://preview-123.vercel.app/api/v1"
    );
    assert_eq!(
        super::resolve_api_url("http://localhost:3000/v1", "http://localhost:8084/")
            .expect("local API"),
        "http://localhost:3000/v1"
    );
    for invalid in [
        "//evil.test/v1",
        "javascript:alert(1)",
        "https://name:secret@example.test/v1",
        "relative/path",
    ] {
        assert!(super::resolve_api_url(invalid, "https://example.test/").is_err());
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn configuration_bootstrap_recovers_temporary_http_failures_without_customer_action() {
    use crate::{api, pages::tests::TestDocument};
    use dioxus::{document::Document, prelude::*};
    use std::rc::Rc;
    let ((settings, scripts), requests) = api::tests::responses(
        &[
            (503, "temporary"),
            (429, "slow down"),
            (200, "[api]\nbase_url = '/api/v1'"),
        ],
        async {
            let page_url = api::request(reqwest::Method::GET, "/nested/cart")
                .expect("fixture page")
                .build()
                .expect("URL")
                .url()
                .to_string();
            let document = TestDocument::new(vec![
                serde_json::json!(page_url),
                serde_json::json!(true),
                serde_json::json!(true),
            ]);
            let dom =
                VirtualDom::new(|| rsx! {}).with_root_context(document.clone() as Rc<dyn Document>);
            let mut operation = std::pin::pin!(super::Settings::new());
            let settings = futures::future::poll_fn(|cx| {
                dom.in_scope(ScopeId::ROOT, || operation.as_mut().poll(cx))
            })
            .await;
            let scripts = document.scripts.borrow().clone();
            (settings, scripts)
        },
    )
    .await;
    assert!(
        settings
            .expect("settings recovered")
            .api
            .base_url
            .ends_with("/api/v1")
    );
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].lines().next(), requests[1].lines().next());
    assert_eq!(requests[1].lines().next(), requests[2].lines().next());
    assert_eq!(scripts.len(), 3);
    assert!(scripts[1].contains("setTimeout(resolve, 5000)"));
    assert!(scripts[2].contains("setTimeout(resolve, 10000)"));
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn malformed_configuration_and_permanent_http_failure_stop_without_waiting() {
    use crate::{api, pages::tests::TestDocument};
    use dioxus::{document::Document, prelude::*};
    use std::rc::Rc;
    for (status, contents) in [
        (404, "missing"),
        (200, "not TOML"),
        (200, "[api]\nbase_url = 'javascript:alert(1)'"),
    ] {
        let ((result, evaluations), requests) =
            api::tests::responses(&[(status, contents)], async {
                let page_url = api::request(reqwest::Method::GET, "/cart")
                    .expect("fixture page")
                    .build()
                    .expect("URL")
                    .url()
                    .to_string();
                let document = TestDocument::new(vec![serde_json::json!(page_url)]);
                let dom = VirtualDom::new(|| rsx! {})
                    .with_root_context(document.clone() as Rc<dyn Document>);
                let mut operation = std::pin::pin!(super::Settings::new());
                let result = futures::future::poll_fn(|cx| {
                    dom.in_scope(ScopeId::ROOT, || operation.as_mut().poll(cx))
                })
                .await;
                let evaluations = document.scripts.borrow().len();
                (result, evaluations)
            })
            .await;
        assert!(
            !result
                .expect_err("terminal configuration error")
                .is_transient()
        );
        assert_eq!(evaluations, 1);
        assert_eq!(requests.len(), 1);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn configuration_bootstrap_recovers_an_interrupted_success_body() {
    use crate::{api, pages::tests::TestDocument};
    use dioxus::{document::Document, prelude::*};
    use std::rc::Rc;
    let contents = "[api]\nbase_url = '/api/v1'";
    let ((result, evaluations), requests) = api::tests::responses_with_lengths(
        &[
            (200, "[api]", contents.len()),
            (200, contents, contents.len()),
        ],
        async {
            let page_url = api::request(reqwest::Method::GET, "/cart")
                .expect("fixture page")
                .build()
                .expect("URL")
                .url()
                .to_string();
            let document =
                TestDocument::new(vec![serde_json::json!(page_url), serde_json::json!(true)]);
            let dom =
                VirtualDom::new(|| rsx! {}).with_root_context(document.clone() as Rc<dyn Document>);
            let mut operation = std::pin::pin!(super::Settings::new());
            let result = futures::future::poll_fn(|cx| {
                dom.in_scope(ScopeId::ROOT, || operation.as_mut().poll(cx))
            })
            .await;
            let evaluations = document.scripts.borrow().len();
            (result, evaluations)
        },
    )
    .await;
    assert!(
        result
            .expect("configuration transfer recovered")
            .api
            .base_url
            .ends_with("/api/v1")
    );
    assert_eq!(requests.len(), 2);
    assert_eq!(evaluations, 2);
}

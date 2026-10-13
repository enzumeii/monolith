use super::*;
use crate::config::{ApiConfig, Settings};
use dioxus::prelude::*;

#[test]
fn requests_use_the_loaded_public_api_base() {
    let dom = VirtualDom::new(|| rsx! {}).with_root_context(Settings {
        api: ApiConfig {
            base_url: "https://api.example.test/custom/v1/".into(),
        },
    });
    let request = dom.in_scope(ScopeId::ROOT, || {
        request(Method::POST, "/auth/refresh")
            .expect("configured request")
            .build()
            .expect("valid request")
    });
    assert_eq!(
        request.url().as_str(),
        "https://api.example.test/custom/v1/auth/refresh"
    );
    assert_eq!(request.headers()["x-session-mode"], "cookie");
}

#[test]
fn requests_fail_before_configuration_is_loaded() {
    let dom = VirtualDom::new(|| rsx! {});
    let error = dom.in_scope(ScopeId::ROOT, || {
        request(Method::GET, "/admins").expect_err("configuration is required")
    });
    assert_eq!(error.status, 503);
}

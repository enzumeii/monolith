use super::*;
use dioxus::prelude::*;

#[test]
fn enable_uses_location_path_and_existing_cookie_transport() {
    let dom = dioxus::prelude::VirtualDom::new(|| dioxus::prelude::rsx! {}).with_root_context(
        crate::config::Settings {
            conventions: crate::config::ConventionsConfig {
                catalogue_location_id: "65761018027".into(),
            },
            api: crate::config::ApiConfig {
                base_url: "https://api.example.test/v1/".into(),
            },
        },
    );
    let request = dom
        .in_scope(dioxus::prelude::ScopeId::ROOT, || {
            enable_request("900719925474099312345")
        })
        .expect("valid ID")
        .build()
        .expect("request");
    assert_eq!(request.method(), Method::POST);
    assert_eq!(
        request.url().as_str(),
        "https://api.example.test/v1/admin/inventories/900719925474099312345/enable"
    );
    assert_eq!(request.headers()["x-session-mode"], "cookie");
    assert!(request.body().is_none());
}

#[test]
fn enable_rejects_invalid_location_ids_without_configuration_or_network() {
    for id in [
        "",
        "0",
        "000",
        "-1",
        "1.5",
        "١٢",
        "12/enable",
        "1?x=2",
        " 12 ",
    ] {
        assert_eq!(enable_request(id).expect_err("invalid ID").status, 400);
    }
}

use super::*;

use crate::config::{ApiConfig, Settings};
use dioxus::prelude::*;

fn configured<T>(call: impl FnOnce() -> T) -> T {
    let dom = VirtualDom::new(|| rsx! {}).with_root_context(Settings {
        conventions: crate::config::ConventionsConfig {
            catalogue_location_id: "65761018027".into(),
        },
        api: ApiConfig {
            base_url: "https://example.test/v1/".into(),
        },
    });
    dom.in_scope(ScopeId::ROOT, call)
}

#[test]
fn list_request_encodes_tags_fulfillment_and_cursor() {
    let request = configured(|| {
        list_request("SHIPPING,FASTSHIP", Some("a+/=&b".into()))
            .expect("valid request")
            .build()
            .expect("built request")
    });
    assert_eq!(request.url().path(), "/v1/admin/orders");
    assert_eq!(request.headers()["x-session-mode"], "cookie");
    assert_eq!(
        request.url().query_pairs().collect::<Vec<_>>(),
        [
            ("tag".into(), "SHIPPING,FASTSHIP".into()),
            ("fulfillment".into(), "UNFULFILLED".into()),
            ("page_size".into(), "250".into()),
            ("cursor".into(), "a+/=&b".into()),
        ]
    );
}

#[test]
fn action_requests_use_the_existing_admin_order_paths() {
    let (labels, fulfill, reship) = configured(|| {
        (
            shipping_labels_request()
                .expect("labels request")
                .build()
                .expect("built labels request"),
            fulfill_request(
                123,
                domain::dto::orders::Courier::Fastship,
                "TRACK123".into(),
            )
            .expect("fulfill request")
            .build()
            .expect("built fulfill request"),
            reship_request("1234")
                .expect("reship request")
                .build()
                .expect("built reship request"),
        )
    });
    assert_eq!(labels.url().path(), "/v1/admin/orders/shipping_label");
    assert_eq!(fulfill.url().path(), "/v1/admin/orders/123/fulfill");
    assert_eq!(reship.url().path(), "/v1/admin/orders/1234/reship");
    assert_eq!(fulfill.method(), reqwest::Method::POST);
    assert_eq!(reship.method(), reqwest::Method::POST);
    assert_eq!(
        fulfill.body().and_then(reqwest::Body::as_bytes),
        Some(&br#"{"courier":"FASTSHIP","tracking_number":"TRACK123"}"#[..])
    );
}

#[test]
fn reship_request_leaves_order_number_validation_to_the_backend() {
    for number in ["999", "10000", "1000", "1234", "9999"] {
        let request = configured(|| {
            reship_request(number)
                .expect("forwarded request")
                .build()
                .expect("built request")
        });
        assert_eq!(
            request.url().path(),
            format!("/v1/admin/orders/{number}/reship")
        );
        assert_eq!(request.method(), reqwest::Method::POST);
    }
}

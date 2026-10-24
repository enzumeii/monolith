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
fn inventory_preserves_opaque_cursor_and_cookie_transport() {
    let request = configured(|| {
        inventory_request(
            "123",
            InventoryQuery {
                page_size: Some(50),
                cursor: Some("a+/=&b".into()),
            },
        )
        .expect("valid test fixture")
        .build()
        .expect("valid test fixture")
    });
    assert_eq!(request.url().path(), "/v1/admin/inventories/123");
    assert_eq!(request.headers()["x-session-mode"], "cookie");
    assert_eq!(
        request.url().query_pairs().collect::<Vec<_>>(),
        [
            ("page_size".into(), "50".into()),
            ("cursor".into(), "a+/=&b".into())
        ]
    );
}
#[test]
fn inventory_defaults_to_fifty_and_rejects_invalid_ids() {
    let request = configured(|| {
        inventory_request(
            "123",
            InventoryQuery {
                page_size: None,
                cursor: None,
            },
        )
        .expect("valid test fixture")
        .build()
        .expect("valid test fixture")
    });
    assert_eq!(request.url().query(), Some("page_size=50"));
    for id in ["", "0", "000", "../orders", "12x"] {
        assert_eq!(
            configured(|| inventory_request(
                id,
                InventoryQuery {
                    page_size: None,
                    cursor: None
                }
            )
            .expect_err("invalid input"))
            .status,
            400
        );
    }
}
#[test]
fn picklist_encodes_tags_without_pagination() {
    let request = configured(|| {
        picklist_request("A & B,C")
            .expect("valid test fixture")
            .build()
            .expect("valid test fixture")
    });
    assert_eq!(request.url().path(), "/v1/admin/orders/report");
    assert_eq!(
        request.url().query_pairs().collect::<Vec<_>>(),
        [("tags".into(), "A & B,C".into())]
    );
    for tags in ["", " ", "a,,b", "a, "] {
        assert_eq!(
            configured(|| picklist_request(tags).expect_err("invalid input")).status,
            400
        );
    }
}

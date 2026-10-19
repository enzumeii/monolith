#![allow(clippy::unwrap_used, clippy::needless_pass_by_value)]
use super::*;
use crate::client::tests::{Fixture, Reply};
use crate::shopify_models::{PageInfo, ProductStatus};
use serde_json::{Value, json};
use uuid::Uuid;
fn id(kind: &str, n: u64) -> ShopifyId {
    ShopifyId::new(format!("gid://shopify/{kind}/{n}")).unwrap()
}
fn client(f: &Fixture) -> ShopifyClient {
    ShopifyClient::new(&ShopifyConfig {
        admin_api_url: f.url.to_string(),
        timeout_secs: 3,
        access_token: Some("private-token".into()),
    })
    .unwrap()
}
fn page(nodes: Vec<Value>, cursor: Option<&str>) -> Value {
    json!({"nodes":nodes,"pageInfo":{"hasNextPage":cursor.is_some(),"endCursor":cursor}})
}
fn money() -> Value {
    json!({"amount":"123.4500","currencyCode":"THB"})
}
fn variant(n: u64) -> Value {
    json!({"id":format!("gid://shopify/ProductVariant/{n}"),"title":"M","sku":null,
        "contextualPricing":{"price":money()},"image":null,
        "inventoryItem":{"id":"gid://shopify/InventoryItem/1","tracked":true,"unitCost":null,"measurement":{"weight":null},"inventoryLevel":null}})
}
fn product(n: u64, variants: Vec<Value>) -> Value {
    json!({"id":format!("gid://shopify/Product/{n}"),"title":"Shirt","description":"Cotton","productType":"KEYCHAIN","status":"ACTIVE","tags":[],"featuredImage":null,"variants":page(variants,None)})
}
fn order(n: u64) -> Value {
    json!({"id":format!("gid://shopify/Order/{n}"),"name":"#1","number":1,"createdAt":"2026-10-01T00:00:00+07:00",
        "displayFinancialStatus":"PENDING","displayFulfillmentStatus":"UNFULFILLED","tags":[],
        "email":null,"phone":null,"shippingAddress":null,
        "currentTotalPriceSet":{"shopMoney":money()},"transactions":[]})
}
fn line(n: u64) -> Value {
    json!({"id":format!("line-{n}"),"title":"Shirt","variantTitle":null,"sku":null,"quantity":3,"currentQuantity":1,
        "product":null,"variant":null,"originalUnitPriceSet":{"shopMoney":money()}})
}
fn sales(n: u64, lines: Value) -> Value {
    json!({"id":format!("gid://shopify/Order/{n}"),"name":"#1","createdAt":"2026-10-01T00:00:00+07:00",
        "cancelledAt":"2026-10-02T00:00:00+07:00","test":true,
        "displayFinancialStatus":"PENDING","displayFulfillmentStatus":"UNFULFILLED","lineItems":lines})
}
fn inventory(n: u64, active: bool, on_hand: i64, tracked: bool) -> Value {
    json!({"id":format!("level-{n}"),"isActive":active,
        "quantities":[{"name":"on_hand","quantity":on_hand},{"name":"available","quantity":on_hand}],
        "item":{"id":format!("gid://shopify/InventoryItem/{n}"),"sku":null,"tracked":tracked,
        "variant":{"id":format!("gid://shopify/ProductVariant/{n}"),"title":"M","product":{"id":"gid://shopify/Product/1","title":"Shirt"}}}})
}
fn child_order(n: u64, lines: Value) -> Value {
    json!({"data":{"order":{"id":format!("gid://shopify/Order/{n}"),"lineItems":lines}}})
}
fn tagged_order(n: u64, lines: Vec<Value>) -> Value {
    json!({"id":format!("gid://shopify/Order/{n}"),"number":n,"tags":["shipping"],"email":null,"phone":null,
        "currentTotalPriceSet":{"shopMoney":money()},"lineItems":page(lines,None)})
}
fn tagged_line() -> Value {
    json!({"title":"Shirt","variantTitle":"Blue","sku":"SHIRT-BLUE","quantity":3,
        "originalUnitPriceSet":{"shopMoney":money()},"product":null,"variant":null})
}
#[test]
fn constructor_trusts_configuration_but_rejects_invalid_url_or_token() {
    let config = ShopifyConfig {
        admin_api_url: "http://localhost:9999/custom/graphql?version=1".into(),
        timeout_secs: 3,
        access_token: Some("token".into()),
    };
    assert!(ShopifyClient::new(&config).unwrap().headers["X-Shopify-Access-Token"].is_sensitive());
    assert!(
        ShopifyClient::new(&ShopifyConfig {
            timeout_secs: 0,
            ..config.clone()
        })
        .is_err()
    );
    assert!(
        ShopifyClient::new(&ShopifyConfig {
            admin_api_url: "not a URL".into(),
            ..config.clone()
        })
        .is_err()
    );
    assert!(
        ShopifyClient::new(&ShopifyConfig {
            access_token: None,
            ..config.clone()
        })
        .is_err()
    );
    assert!(
        ShopifyClient::new(&ShopifyConfig {
            access_token: Some("   ".into()),
            ..config.clone()
        })
        .is_err()
    );
    assert!(
        ShopifyClient::new(&ShopifyConfig {
            access_token: Some("bad\nheader".into()),
            ..config
        })
        .is_err()
    );
}

#[tokio::test]
async fn mutations_return_raw_user_errors_and_optional_outputs_without_replay() {
    let f=Fixture::new(vec![
        Reply::json(json!({"data":{"orderUpdate":{"order":null,"userErrors":[{"field":null,"message":"denied"}]}}})),
        Reply::json(json!({"data":{"inventoryActivate":{"inventoryLevel":null,"userErrors":[{"field":null,"message":"denied"}]}}})),
    ]).await;
    let c = client(&f);
    let key = Uuid::new_v4();
    let tags = c
        .replace_order_tags(&ReplaceOrderTagsRequest {
            order_id: id("Order", 1),
            tags: vec![],
        })
        .await
        .unwrap()
        .order_update;
    assert!(tags.order.is_none());
    assert_eq!(tags.user_errors.len(), 1);
    let activation = c
        .activate_existing_inventory(&ActivateInventoryRequest {
            inventory_item_id: id("InventoryItem", 1),
            location_id: id("Location", 1),
            idempotency_key: key,
        })
        .await
        .unwrap()
        .inventory_activate;
    assert!(activation.inventory_level.is_none());
    assert_eq!(activation.user_errors.len(), 1);
    let requests = f.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].2["variables"]["input"]["tags"], json!([]));
    assert_eq!(
        requests[1].2["variables"]["idempotencyKey"],
        key.to_string()
    );
    assert!(requests[1].2["variables"].get("available").is_none());
    assert!(requests[1].2["variables"].get("onHand").is_none());
}

#[tokio::test]
async fn activation_omits_quantities_and_preserves_idempotency_key_on_throttling() {
    let f=Fixture::new(vec![Reply::status(429),Reply::json(json!({"data":{"inventoryActivate":{
        "inventoryLevel":{"id":"level-1","isActive":true,"quantities":[{"name":"available","quantity":1},{"name":"on_hand","quantity":1}]},"userErrors":[]
    }}}))]).await;
    let key = Uuid::parse_str("79c99261-13df-45c7-818c-f24b111a75c9").unwrap();
    let level = client(&f)
        .activate_existing_inventory(&ActivateInventoryRequest {
            inventory_item_id: id("InventoryItem", 1),
            location_id: id("Location", 1),
            idempotency_key: key,
        })
        .await
        .unwrap()
        .inventory_activate
        .inventory_level
        .unwrap();
    assert!(level.is_active);
    assert_eq!(level.quantities[0].quantity, 1);
    let requests = f.requests();
    assert_eq!(
        requests[0].2["variables"]["idempotencyKey"],
        key.to_string()
    );
    assert_eq!(requests[0].2, requests[1].2);
    assert!(
        !requests[0].2["query"]
            .as_str()
            .unwrap()
            .contains("available:")
    );
    assert!(!requests[0].2["query"].as_str().unwrap().contains("onHand:"));
    for (_, _, request) in requests {
        let variables = request["variables"].as_object().unwrap();
        assert_eq!(variables.len(), 3);
        assert!(variables.get("onHand").is_none());
        assert!(variables.get("available").is_none());
        let query = request["query"].as_str().unwrap();
        assert!(!query.contains("$onHand") && !query.contains("$available"));
    }
}

#[tokio::test]
async fn location_lookup_has_no_catalog_pricing_dependency() {
    for exists in [false, true] {
        let location = if exists {
            json!({"id":id("Location",1)})
        } else {
            Value::Null
        };
        let f = Fixture::new(vec![Reply::json(json!({"data":{"location":location}}))]).await;
        assert_eq!(
            client(&f)
                .location_exists(&id("Location", 1))
                .await
                .unwrap(),
            exists
        );
        assert_eq!(f.requests().len(), 1);
        assert!(
            !f.requests()[0].2["query"]
                .as_str()
                .unwrap()
                .contains("contextualPricing")
        );
    }
}

#[tokio::test]
async fn proxy_returns_one_parent_page_without_following_root_or_child_cursors() {
    let mut first = product(1, vec![variant(1)]);
    first["variants"] = page(vec![variant(1)], Some("child-next"));
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"products":page(vec![first],Some("parent-next"))}}),
    )])
    .await;
    let result = client(&f)
        .products_with_variants(
            &ListProductsRequest {
                location_id: id("Location", 1),
                statuses: vec![ProductStatus::Active],
            },
            &request_page(),
        )
        .await
        .unwrap();
    assert_eq!(result.nodes.len(), 1);
    assert_eq!(result.page_info.end_cursor.as_deref(), Some("parent-next"));
    assert!(result.nodes[0].variants.page_info.has_next_page);
    let requests = f.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].2["variables"],
        json!({"locationId":id("Location",1),"search":"status:active","first":7,"after":"incoming"})
    );
}

#[tokio::test]
async fn tag_lookup_escapes_each_literal_and_requires_every_tag() {
    for (tags, search) in [
        (vec!["shipping"], r#"tag:"shipping""#),
        (vec![r#"a"b\c:(d)"#], r#"tag:"a\"b\\c\:\(d\)""#),
        (vec!["SHIPPING", "TEST"], r#"tag:"SHIPPING" AND tag:"TEST""#),
    ] {
        let f = Fixture::new(vec![Reply::json(
            json!({"data":{"orders":page(vec![],None)}}),
        )])
        .await;
        assert!(
            client(&f)
                .orders_by_tag(
                    &OrdersByTagRequest {
                        tags: tags.into_iter().map(str::to_owned).collect()
                    },
                    &request_page()
                )
                .await
                .unwrap()
                .nodes
                .is_empty()
        );
        assert_eq!(f.requests()[0].2["variables"]["search"], search);
    }
}

#[tokio::test]
async fn proxy_preserves_typed_upstream_failures() {
    for (kind, reply) in [
        ("http", Reply::status(503)),
        (
            "graphql",
            Reply::json(json!({"data":null,"errors":[{"message":"private"}]})),
        ),
        (
            "decode",
            Reply {
                status: 200,
                body: "{".into(),
                headers: vec![],
            },
        ),
    ] {
        let f = Fixture::new(vec![reply]).await;
        let error = client(&f)
            .orders_by_tag(
                &OrdersByTagRequest {
                    tags: vec!["tag".into()],
                },
                &request_page(),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error.location().file(), "services/api/src/client.rs");
        assert!(matches!(
            (kind, error.kind()),
            ("http", ClientErrorKind::Http { status: 503, .. })
                | ("graphql", ClientErrorKind::Graphql { .. })
                | ("decode", ClientErrorKind::Decode { .. })
        ));
    }
}
#[tokio::test]
async fn bounded_product_query_uses_fixed_250_variants() {
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"products":page(vec![],None)}}),
    )])
    .await;
    client(&f)
        .products_with_variants(
            &ListProductsRequest {
                location_id: id("Location", 1),
                statuses: vec![ProductStatus::Active],
            },
            &request_page(),
        )
        .await
        .unwrap();
    let requests = f.requests();
    let query = requests[0].2["query"].as_str().unwrap();
    assert!(query.contains("variants(first: 250)"));
    assert!(requests[0].2["variables"].get("variantsFirst").is_none());
}

fn request_page() -> PageRequest {
    PageRequest {
        first: 7,
        after: Some("incoming".into()),
    }
}
#[tokio::test]
async fn root_proxies_forward_pages_without_collecting_them() {
    for operation in [
        "OrdersByPayment",
        "OrdersByTag",
        "SoldVariantsInRange",
        "FindInventoryToActivate",
    ] {
        let nodes = match operation {
            "OrdersByPayment" => vec![order(1)],
            "OrdersByTag" => vec![tagged_order(1, vec![tagged_line()])],
            "SoldVariantsInRange" => vec![sales(1, page(vec![], None))],
            _ => vec![inventory(1, false, 0, true)],
        };
        let connection = page(nodes, Some("next"));
        let response = if operation == "FindInventoryToActivate" {
            json!({"data":{"location":{"inventoryLevels":connection}}})
        } else {
            json!({"data":{"orders":connection}})
        };
        let f = Fixture::new(vec![Reply::json(response)]).await;
        let c = client(&f);
        let p = request_page();
        let (count, info) = call_root_proxy(&c, operation, &p).await;
        assert_eq!(count, 1);
        assert_eq!(info.end_cursor.as_deref(), Some("next"));
        assert!(info.has_next_page);
        let requests = f.requests();
        assert_eq!(requests.len(), 1);
        let request = &requests[0].2;
        let variables = &request["variables"];
        assert_eq!(request["operationName"], operation);
        assert_eq!(variables["first"], 7);
        assert_eq!(
            variables[if operation == "SoldVariantsInRange" {
                "ordersAfter"
            } else {
                "after"
            }],
            "incoming"
        );
        assert!(variables.get("linesFirst").is_none());
        if operation == "OrdersByTag" {
            assert!(
                request["query"]
                    .as_str()
                    .unwrap()
                    .contains("lineItems(first: 250)")
            );
        }
    }
}
#[tokio::test]
async fn bounded_sales_lines_accept_only_order_id() {
    let f = Fixture::new(vec![Reply::json(child_order(1, page(vec![line(1)], None)))]).await;
    let result = client(&f)
        .sales_order_lines(&OrderLinesRequest {
            order_id: id("Order", 1),
        })
        .await
        .unwrap()
        .order
        .unwrap();
    assert_eq!(result.id, id("Order", 1));
    assert_eq!(result.line_items.nodes[0].quantity, 3);
    let requests = f.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].2["variables"], json!({"id":id("Order",1)}));
    let query = requests[0].2["query"].as_str().unwrap();
    assert!(query.contains("$includeLines: Boolean! = true"));
    assert!(query.contains("$includeContact: Boolean! = false"));
    assert!(query.contains("lineItems(first: 250) @include(if: $includeLines)"));
    assert!(query.contains("number @include(if: $includeContact)"));
    assert!(query.contains("email @include(if: $includeContact)"));
    assert!(
        requests[0].2["query"]
            .as_str()
            .unwrap()
            .contains("lineItems(first: 250)")
    );
}
#[tokio::test]
async fn invalid_client_inputs_are_rejected_before_network() {
    let f = Fixture::new(vec![]).await;
    let c = client(&f);
    let p = request_page();
    for error in [
        c.orders_by_tag(
            &OrdersByTagRequest {
                tags: vec![" ".into()],
            },
            &p,
        )
        .await
        .err()
        .unwrap(),
        c.sales_order_lines(&OrderLinesRequest {
            order_id: id("Location", 1),
        })
        .await
        .err()
        .unwrap(),
        c.orders_by_payment(
            &OrdersByPaymentRequest {
                payment_id: "pay".into(),
            },
            &PageRequest {
                first: 0,
                after: None,
            },
        )
        .await
        .err()
        .unwrap(),
    ] {
        assert!(matches!(error.kind(), ClientErrorKind::InvalidInput(_)));
    }
    assert_eq!(f.requests().len(), 0);
}
async fn call_root_proxy(
    c: &ShopifyClient,
    operation: &str,
    page: &PageRequest,
) -> (usize, PageInfo) {
    match operation {
        "OrdersByPayment" => {
            let p = c
                .orders_by_payment(
                    &OrdersByPaymentRequest {
                        payment_id: "pay".into(),
                    },
                    page,
                )
                .await
                .unwrap();
            (p.nodes.len(), p.page_info)
        }
        "OrdersByTag" => {
            let p = c
                .orders_by_tag(
                    &OrdersByTagRequest {
                        tags: vec!["shipping".into()],
                    },
                    page,
                )
                .await
                .unwrap();
            assert_eq!(p.nodes[0].line_items.nodes.len(), 1);
            (p.nodes.len(), p.page_info)
        }
        "SoldVariantsInRange" => {
            let p = c
                .sold_variants_in_range(
                    &SalesRangeRequest {
                        start: "2026-01-01T00:00:00Z".parse().unwrap(),
                        end: "2026-02-01T00:00:00Z".parse().unwrap(),
                    },
                    page,
                )
                .await
                .unwrap();
            (p.nodes.len(), p.page_info)
        }
        _ => {
            let p = c
                .find_inventory_to_activate(
                    &InventoryDiscoveryRequest {
                        location_id: id("Location", 1),
                    },
                    page,
                )
                .await
                .unwrap();
            let p = p.unwrap();
            assert!(!p.nodes[0].is_active);
            (p.nodes.len(), p.page_info)
        }
    }
}

#[tokio::test]
async fn inventory_discovery_distinguishes_missing_location_key_from_null() {
    let f = Fixture::new(vec![
        Reply::json(json!({"data":{}})),
        Reply::json(json!({"data":{"location":null}})),
    ])
    .await;
    let c = client(&f);
    let input = InventoryDiscoveryRequest {
        location_id: id("Location", 1),
    };
    let page = PageRequest {
        first: 250,
        after: None,
    };
    assert!(c.find_inventory_to_activate(&input, &page).await.is_err());
    assert!(
        c.find_inventory_to_activate(&input, &page)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(f.requests().len(), 2);
}

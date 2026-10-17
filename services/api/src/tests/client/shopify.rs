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
fn tagged_lines(first: u64, count: u64) -> Vec<Value> {
    (first..first + count)
        .map(|n| {
            let mut line = tagged_line();
            line["title"] = json!(format!("line-{n}"));
            line
        })
        .collect()
}

#[test]
fn constructor_trusts_configuration_but_rejects_invalid_url_or_token() {
    let config = ShopifyConfig {
        admin_api_url: "http://localhost:9999/custom/graphql?version=1".into(),
        access_token: Some("token".into()),
    };
    assert!(ShopifyClient::new(&config).unwrap().headers["X-Shopify-Access-Token"].is_sensitive());
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
async fn activation_preserves_quantities_and_idempotency_key_on_throttling() {
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

fn request_page() -> PageRequest {
    PageRequest {
        first: 7,
        after: Some("incoming".into()),
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
                variants_first: 3,
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
        json!({"locationId":id("Location",1),"search":"status:active","first":7,"after":"incoming","variantsFirst":3})
    );
}
#[tokio::test]
async fn every_paginated_proxy_forwards_size_cursor_and_returns_uncollected_page() {
    for operation in [
        "OrdersByPayment",
        "OrdersByTag",
        "SoldVariantsInRange",
        "SalesOrderLines",
        "TaggedOrderLines",
        "ProductVariants",
        "FindInventoryToActivate",
    ] {
        let response = paginated_response(operation);
        let f = Fixture::new(vec![Reply::json(response)]).await;
        let c = client(&f);
        let page = request_page();
        let (count, info) = call_paginated_proxy(&c, operation, &page).await;
        assert_eq!(count, 1);
        assert_eq!(info.end_cursor.as_deref(), Some("next"));
        assert!(info.has_next_page);
        let requests = f.requests();
        assert_eq!(requests.len(), 1);
        let v = &requests[0].2["variables"];
        assert_eq!(v["first"], 7);
        assert_eq!(
            v[if operation == "SoldVariantsInRange" {
                "ordersAfter"
            } else {
                "after"
            }],
            "incoming"
        );
        assert_eq!(requests[0].2["operationName"], operation);
        if operation == "OrdersByTag" {
            assert_eq!(v["linesFirst"], 4);
        }
    }
}
#[tokio::test]
async fn invalid_client_inputs_are_rejected_before_network() {
    let f = Fixture::new(vec![]).await;
    let c = client(&f);
    let p = request_page();
    for error in [
        c.orders_by_tag(
            &OrdersByTagRequest {
                tag: " ".into(),
                lines_first: 10,
            },
            &p,
        )
        .await
        .err()
        .unwrap(),
        c.sales_order_lines(
            &OrderLinesRequest {
                order_id: id("Location", 1),
            },
            &p,
        )
        .await
        .err()
        .unwrap(),
        c.product_variants(
            &ProductVariantsRequest {
                product_id: id("Order", 1),
                location_id: id("Location", 1),
            },
            &p,
        )
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
        assert!(matches!(
            error,
            ClientError::Origin {
                origin: "api::client::shopify",
                ..
            }
        ));
    }
    assert_eq!(f.requests().len(), 0);
}
#[tokio::test]
async fn tag_lookup_escapes_one_literal_instead_of_accepting_search_syntax() {
    for (tag, search) in [
        ("shipping", r#"tag:"shipping""#),
        (r#"a"b\c:(d)"#, r#"tag:"a\"b\\c\:\(d\)""#),
    ] {
        let f = Fixture::new(vec![Reply::json(
            json!({"data":{"orders":page(vec![],None)}}),
        )])
        .await;
        assert!(
            client(&f)
                .orders_by_tag(
                    &OrdersByTagRequest {
                        tag: tag.into(),
                        lines_first: 10
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
                    tag: "tag".into(),
                    lines_first: 10,
                },
                &request_page(),
            )
            .await
            .err()
            .unwrap();
        let ClientError::Origin { origin, source } = error else {
            panic!("origin missing")
        };
        assert_eq!(origin, "api::client::shopify");
        assert!(matches!(
            (kind, *source),
            ("http", ClientError::Http { status: 503, .. })
                | ("graphql", ClientError::Graphql { .. })
                | ("decode", ClientError::Decode { .. })
        ));
    }
}
fn paginated_response(operation: &str) -> Value {
    let connection = match operation {
        "OrdersByPayment" => page(vec![order(1)], Some("next")),
        "OrdersByTag" => page(vec![tagged_order(1, vec![tagged_line()])], Some("next")),
        "SoldVariantsInRange" => page(vec![sales(1, page(vec![], None))], Some("next")),
        "SalesOrderLines" => page(vec![line(1)], Some("next")),
        "TaggedOrderLines" => page(tagged_lines(1, 1), Some("next")),
        "ProductVariants" => page(vec![variant(1)], Some("next")),
        _ => page(vec![inventory(1, false, 0, true)], Some("next")),
    };
    match operation {
        "SalesOrderLines" | "TaggedOrderLines" => child_order(1, connection),
        "ProductVariants" => {
            json!({"data":{"product":{"id":id("Product",1),"variants":connection}}})
        }
        "FindInventoryToActivate" => {
            json!({"data":{"location":{"inventoryLevels":connection}}})
        }
        _ => json!({"data":{"orders":connection}}),
    }
}
async fn call_paginated_proxy(
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
                        tag: "shipping".into(),
                        lines_first: 4,
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
        "SalesOrderLines" => {
            let p = c
                .sales_order_lines(
                    &OrderLinesRequest {
                        order_id: id("Order", 1),
                    },
                    page,
                )
                .await
                .unwrap()
                .into_connection(&id("Order", 1))
                .unwrap();
            (p.nodes.len(), p.page_info)
        }
        "TaggedOrderLines" => {
            let p = c
                .tagged_order_lines(
                    &OrderLinesRequest {
                        order_id: id("Order", 1),
                    },
                    page,
                )
                .await
                .unwrap()
                .into_connection(&id("Order", 1))
                .unwrap();
            (p.nodes.len(), p.page_info)
        }
        "ProductVariants" => {
            let p = c
                .product_variants(
                    &ProductVariantsRequest {
                        product_id: id("Product", 1),
                        location_id: id("Location", 1),
                    },
                    page,
                )
                .await
                .unwrap()
                .into_connection(&id("Product", 1))
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
            assert!(!p.nodes[0].is_active);
            (p.nodes.len(), p.page_info)
        }
    }
}

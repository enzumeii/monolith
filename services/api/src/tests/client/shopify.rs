#![allow(clippy::unwrap_used, clippy::needless_pass_by_value)]
use super::*;
use crate::{
    client::tests::{Fixture, Reply},
    shopify_models::*,
};
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
        "product":null,"variant":null,"originalUnitPriceSet":{"shopMoney":money()},"product":null,"variant":null})
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
        "currentTotalPriceSet":{"shopMoney":money()},"lineItems":{"nodes":lines}})
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
async fn catalog_collects_all_parents_and_outer_pages() {
    let f = Fixture::new(vec![
        Reply::json(json!({"data":{"products":page(vec![
            product(1,vec![variant(1);250]),
            product(2,vec![variant(2)])],Some("p1"))}})),
        Reply::json(json!({"data":{"products":page(vec![product(3,vec![])],None)}})),
    ])
    .await;
    let result = client(&f)
        .products_with_variants(&ListProductsRequest {
            location_id: id("Location", 1),
            statuses: vec![ProductStatus::Active, ProductStatus::Draft],
        })
        .await
        .unwrap();
    assert_eq!(
        result.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
        vec![id("Product", 1), id("Product", 2), id("Product", 3)]
    );
    assert_eq!(result[0].variants.nodes.len(), 250);
    assert!(result[2].variants.nodes.is_empty());
    assert_eq!(
        result[0].variants.nodes[0]
            .contextual_pricing
            .price
            .currency_code,
        "THB"
    );
    let requests = f.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].2["variables"],
        json!({"locationId":id("Location",1),"search":"status:active,draft","after":null})
    );
    assert_eq!(requests[1].2["variables"]["after"], "p1");
    let query = requests[0].2["query"].as_str().unwrap();
    assert!(query.contains("products(first: 10"));
    assert!(query.contains("variants(first: 10)"));
}

#[tokio::test]
async fn catalog_complete_variant_page_needs_no_child_request() {
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"products":page(vec![product(1,vec![variant(1);250])],None)}}),
    )])
    .await;
    let products = client(&f)
        .products_with_variants(&ListProductsRequest {
            location_id: id("Location", 1),
            statuses: vec![],
        })
        .await
        .unwrap();
    assert_eq!(products[0].variants.nodes.len(), 250);
    assert_eq!(f.requests().len(), 1);
    assert!(
        f.requests()[0].2["variables"]
            .get("variantsAfter")
            .is_none()
    );
}

#[tokio::test]
async fn payment_lookup_escapes_literal_and_collects_all_matches() {
    let f = Fixture::new(vec![
        Reply::json(json!({"data":{"orders":page(vec![order(1)],Some("a"))}})),
        Reply::json(json!({"data":{"orders":page(vec![order(2)],None)}})),
    ])
    .await;
    let orders = client(&f)
        .orders_by_payment(&OrdersByPaymentRequest {
            payment_id: "a\"b\\c:(d)".into(),
        })
        .await
        .unwrap();
    assert_eq!(orders.len(), 2);
    assert_eq!(
        f.requests()[0].2["variables"]["search"],
        "payment_id:\"a\\\"b\\\\c\\:\\(d\\)\""
    );
}

#[tokio::test]
async fn empty_results_are_not_errors_and_invalid_inputs_never_call_shopify() {
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"orders":page(vec![],None)}}),
    )])
    .await;
    assert_eq!(
        client(&f)
            .orders_by_payment(&OrdersByPaymentRequest {
                payment_id: "ref".into()
            })
            .await
            .unwrap()
            .len(),
        0
    );
    let f = Fixture::new(vec![]).await;
    assert!(
        client(&f)
            .products_with_variants(&ListProductsRequest {
                location_id: id("Order", 1),
                statuses: vec![]
            })
            .await
            .is_err()
    );
    assert!(
        client(&f)
            .orders_by_payment(&OrdersByPaymentRequest {
                payment_id: String::new()
            })
            .await
            .is_err()
    );
    assert!(
        client(&f)
            .sold_variants_in_range(&SalesRangeRequest {
                start: "2026-10-01T00:00:00Z".parse().unwrap(),
                end: "2026-10-01T00:00:00Z".parse().unwrap()
            })
            .await
            .is_err()
    );
    assert_eq!(f.requests().len(), 0);
}

fn cause(error: &ClientError) -> &ClientError {
    match error {
        ClientError::Origin { origin, source } => {
            assert_eq!(*origin, "api::client::shopify");
            source
        }
        _ => panic!("missing Shopify origin: {error:?}"),
    }
}
#[tokio::test]
async fn order_header_queries_paginate_independently_without_loading_lines() {
    for tagged in [false, true] {
        let headers = |n| {
            if tagged {
                tagged_order(n, vec![])
            } else {
                sales(n, page(vec![], None))
            }
        };
        let f = Fixture::new(vec![
            Reply::json(json!({"data":{"orders":page(vec![headers(1),headers(2)],Some("next"))}})),
            Reply::json(json!({"data":{"orders":page(vec![headers(3)],None)}})),
        ])
        .await;
        let ids = if tagged {
            let result = client(&f)
                .orders_by_tag(&OrdersByTagRequest {
                    tag: "shipping".into(),
                })
                .await
                .unwrap();
            assert_eq!(
                result[0].current_total_price_set.shop_money.currency_code,
                "THB"
            );
            result.into_iter().map(|o| o.id).collect::<Vec<_>>()
        } else {
            let result = client(&f)
                .sold_variants_in_range(&SalesRangeRequest {
                    start: "2026-01-01T00:00:00+07:00".parse().unwrap(),
                    end: "2026-02-01T00:00:00+07:00".parse().unwrap(),
                })
                .await
                .unwrap();
            assert!(result[0].test);
            assert!(result[0].cancelled_at.is_some());
            result.into_iter().map(|o| o.id).collect::<Vec<_>>()
        };
        assert_eq!(ids, vec![id("Order", 1), id("Order", 2), id("Order", 3)]);
        let requests = f.requests();
        assert_eq!(requests.len(), 2);
        let (operation, cursor) = if tagged {
            ("OrdersByTag", "after")
        } else {
            ("SoldVariantsInRange", "ordersAfter")
        };
        assert_eq!(requests[0].2["operationName"], operation);
        assert_eq!(requests[0].2["variables"][cursor], Value::Null);
        assert_eq!(requests[1].2["variables"][cursor], "next");
        assert!(
            !requests[0].2["query"]
                .as_str()
                .unwrap()
                .contains("lineItems")
        );
    }
}
#[tokio::test]
async fn line_queries_collect_501_distinct_nodes_without_root_order_searches() {
    for tagged in [false, true] {
        let lines = |first, count| {
            if tagged {
                tagged_lines(first, count)
            } else {
                (first..first + count).map(line).collect()
            }
        };
        let f = Fixture::new(vec![
            Reply::json(child_order(1, page(lines(1, 250), Some("a")))),
            Reply::json(child_order(1, page(lines(251, 250), Some("b")))),
            Reply::json(child_order(1, page(lines(501, 1), None))),
        ])
        .await;
        let input = OrderLinesRequest {
            order_id: id("Order", 1),
        };
        let labels = if tagged {
            let result = client(&f).tagged_order_lines(&input).await.unwrap();
            assert_eq!(
                result[500].original_unit_price_set.shop_money.currency_code,
                "THB"
            );
            result.into_iter().map(|l| l.title).collect::<Vec<_>>()
        } else {
            let result = client(&f).sales_order_lines(&input).await.unwrap();
            assert_eq!(
                result[500].original_unit_price_set.shop_money.currency_code,
                "THB"
            );
            assert_eq!(result[500].current_quantity, 1);
            result.into_iter().map(|l| l.id).collect::<Vec<_>>()
        };
        assert_eq!(labels.len(), 501);
        assert_eq!(labels[250], "line-251");
        assert_eq!(labels[500], "line-501");
        let requests = f.requests();
        assert_eq!(requests.len(), 3);
        for (index, after) in [Value::Null, json!("a"), json!("b")]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                requests[index].2["variables"],
                json!({"id":id("Order",1),"after":after})
            );
            assert!(
                requests[index].2["query"]
                    .as_str()
                    .unwrap()
                    .contains(if tagged {
                        "lineItems(first: 100, after: $after)"
                    } else {
                        "lineItems(first: 250, after: $after)"
                    })
            );
            assert!(
                !requests[index].2["query"]
                    .as_str()
                    .unwrap()
                    .contains("orders(")
            );
        }
    }
}
#[tokio::test]
async fn inventory_collects_all_raw_levels_without_business_filtering() {
    let f=Fixture::new(vec![
        Reply::json(json!({"data":{"location":{"inventoryLevels":page(vec![inventory(1,true,3,true),inventory(2,false,0,true)],Some("next"))}}})),
        Reply::json(json!({"data":{"location":{"inventoryLevels":page(vec![inventory(3,false,4,false),inventory(4,false,5,true)],None)}}})),
    ]).await;
    let levels = client(&f)
        .find_inventory_to_activate(&InventoryDiscoveryRequest {
            location_id: id("Location", 1),
        })
        .await
        .unwrap();
    assert_eq!(
        levels.iter().map(|l| l.id.as_str()).collect::<Vec<_>>(),
        vec!["level-1", "level-2", "level-3", "level-4"]
    );
    assert_eq!(f.requests().len(), 2);
    assert_eq!(f.requests()[1].2["variables"]["after"], "next");
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
async fn invalid_ids_and_blank_literals_have_shopify_origin_before_network() {
    let f = Fixture::new(vec![]).await;
    let c = client(&f);
    let input = OrderLinesRequest {
        order_id: id("Location", 1),
    };
    for error in [
        c.sales_order_lines(&input).await.err().unwrap(),
        c.tagged_order_lines(&input).await.err().unwrap(),
        c.orders_by_tag(&OrdersByTagRequest { tag: " \t".into() })
            .await
            .err()
            .unwrap(),
        c.replace_order_tags(&ReplaceOrderTagsRequest {
            order_id: id("Location", 1),
            tags: vec![],
        })
        .await
        .err()
        .unwrap(),
        c.activate_existing_inventory(&ActivateInventoryRequest {
            inventory_item_id: id("Order", 1),
            location_id: id("Location", 1),
            idempotency_key: Uuid::new_v4(),
        })
        .await
        .err()
        .unwrap(),
    ] {
        assert!(matches!(cause(&error), ClientError::InvalidInput(_)));
    }
    assert_eq!(f.requests().len(), 0);
}
#[tokio::test]
async fn root_and_child_cursors_reject_missing_empty_repeated_and_cyclic_values() {
    for child in [false, true] {
        for cursors in [
            vec![None],
            vec![Some("")],
            vec![Some("a"), Some("a")],
            vec![Some("a"), Some("b"), Some("a")],
        ] {
            let replies = cursors
                .iter()
                .map(|cursor| {
                    let mut connection = page(vec![], *cursor);
                    connection["pageInfo"]["hasNextPage"] = json!(true);
                    Reply::json(if child {
                        child_order(1, connection)
                    } else {
                        json!({"data":{"orders":connection}})
                    })
                })
                .collect();
            let f = Fixture::new(replies).await;
            let c = client(&f);
            let error = if child {
                c.sales_order_lines(&OrderLinesRequest {
                    order_id: id("Order", 1),
                })
                .await
                .err()
                .unwrap()
            } else {
                c.orders_by_tag(&OrdersByTagRequest {
                    tag: "shipping".into(),
                })
                .await
                .err()
                .unwrap()
            };
            assert!(matches!(cause(&error), ClientError::Protocol(_)));
            assert_eq!(f.requests().len(), cursors.len());
        }
    }
}
#[tokio::test]
async fn missing_or_wrong_parents_and_empty_children_keep_protocol_contract() {
    for tagged in [false, true] {
        for parent in [
            json!({"data":{"order":null}}),
            child_order(99, page(vec![], None)),
        ] {
            let f = Fixture::new(vec![Reply::json(parent)]).await;
            let c = client(&f);
            let input = OrderLinesRequest {
                order_id: id("Order", 1),
            };
            let error = if tagged {
                c.tagged_order_lines(&input).await.err().unwrap()
            } else {
                c.sales_order_lines(&input).await.err().unwrap()
            };
            assert!(matches!(cause(&error), ClientError::Protocol(_)));
        }
        let f = Fixture::new(vec![Reply::json(child_order(1, page(vec![], None)))]).await;
        let c = client(&f);
        let input = OrderLinesRequest {
            order_id: id("Order", 1),
        };
        assert_eq!(
            if tagged {
                c.tagged_order_lines(&input).await.unwrap().len()
            } else {
                c.sales_order_lines(&input).await.unwrap().len()
            },
            0
        );
    }
    let f = Fixture::new(vec![Reply::json(json!({"data":{"location":null}}))]).await;
    let error = client(&f)
        .find_inventory_to_activate(&InventoryDiscoveryRequest {
            location_id: id("Location", 1),
        })
        .await
        .err()
        .unwrap();
    assert!(matches!(
        cause(&error),
        ClientError::InvalidInput("location was not found")
    ));
}
#[tokio::test]
async fn later_http_graphql_or_decode_failures_never_return_partial_results() {
    for child in [false, true] {
        for kind in ["http", "graphql", "decode"] {
            let first = if child {
                child_order(1, page(vec![line(1)], Some("next")))
            } else {
                json!({"data":{"orders":page(vec![tagged_order(1,vec![])],Some("next"))}})
            };
            let failure = match kind {
                "http" => Reply::status(503),
                "graphql" => {
                    Reply::json(json!({"data":null,"errors":[{"message":"private-payload"}]}))
                }
                _ => Reply {
                    status: 200,
                    body: "{".into(),
                    headers: vec![],
                },
            };
            let f = Fixture::new(vec![Reply::json(first), failure]).await;
            let c = client(&f);
            let error = if child {
                c.sales_order_lines(&OrderLinesRequest {
                    order_id: id("Order", 1),
                })
                .await
                .err()
                .unwrap()
            } else {
                c.orders_by_tag(&OrdersByTagRequest {
                    tag: "shipping".into(),
                })
                .await
                .err()
                .unwrap()
            };
            match (kind, cause(&error)) {
                ("http", ClientError::Http { status: 503, .. })
                | ("graphql", ClientError::Graphql { .. })
                | ("decode", ClientError::Decode { .. }) => {}
                _ => panic!("typed source lost: {error:?}"),
            }
            assert_eq!(f.requests().len(), 2);
        }
    }
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
async fn tag_lookup_escapes_one_literal_instead_of_accepting_search_syntax() {
    for (tag, search) in [
        ("shipping", r#"tag:"shipping""#),
        (r#"a"b\c:(d)"#, r#"tag:"a\"b\\c\:\(d\)""#),
    ] {
        let f = Fixture::new(vec![Reply::json(
            json!({"data":{"orders":page(vec![],None)}}),
        )])
        .await;
        assert_eq!(
            client(&f)
                .orders_by_tag(&OrdersByTagRequest { tag: tag.into() })
                .await
                .unwrap()
                .len(),
            0
        );
        assert_eq!(f.requests()[0].2["variables"]["search"], search);
    }
}

#[tokio::test]
async fn catalog_collects_root_and_variant_pages() {
    let p = |n, cursor| {
        let mut value = product(n, vec![variant(n)]);
        value["variants"] = page(vec![variant(n)], cursor);
        value
    };
    let f=Fixture::new(vec![
        Reply::json(json!({"data":{"products":page(vec![p(1,Some("v1"))],Some("p1"))}})),
        Reply::json(json!({"data":{"products":page(vec![p(2,None)],None)}})),
        Reply::json(json!({"data":{"product":{"id":id("Product",1),"variants":page(vec![variant(3)],None)}}})),
    ]).await;
    let products = client(&f)
        .products_with_variants(&ListProductsRequest {
            location_id: id("Location", 1),
            statuses: vec![ProductStatus::Active],
        })
        .await
        .unwrap();
    assert_eq!(products.len(), 2);
    assert_eq!(products[0].variants.nodes.len(), 2);
    let requests = f.requests();
    assert_eq!(requests[1].2["variables"]["after"], "p1");
    assert_eq!(requests[2].2["variables"]["after"], "v1");
    assert_eq!(requests[0].2["variables"]["search"], "status:active");
}

#[tokio::test]
async fn variant_failure_never_returns_partial_catalog() {
    for parent in [
        json!(null),
        json!({"id":id("Product",99),"variants":page(vec![],None)}),
        json!({"id":id("Product",1),"variants":page(vec![],Some("again"))}),
    ] {
        let mut first = product(1, vec![variant(1)]);
        first["variants"] = page(vec![variant(1)], Some("again"));
        let f = Fixture::new(vec![
            Reply::json(json!({"data":{"products":page(vec![first],None)}})),
            Reply::json(json!({"data":{"product":parent}})),
        ])
        .await;
        assert!(
            client(&f)
                .products_with_variants(&ListProductsRequest {
                    location_id: id("Location", 1),
                    statuses: vec![ProductStatus::Active]
                })
                .await
                .is_err()
        );
        assert_eq!(f.requests().len(), 2);
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
async fn existing_catalog_method_collects_child_variant_pages() {
    let mut first = product(1, vec![variant(1)]);
    first["variants"]["pageInfo"] = json!({"hasNextPage":true,"endCursor":"child"});
    let f=Fixture::new(vec![
        Reply::json(json!({"data":{"products":page(vec![first],None)}})),
        Reply::json(json!({"data":{"product":{"id":id("Product",1),"variants":page(vec![variant(2)],None)}}})),
    ]).await;
    let products = client(&f)
        .products_with_variants(&ListProductsRequest {
            location_id: id("Location", 1),
            statuses: vec![ProductStatus::Active],
        })
        .await
        .unwrap();
    assert_eq!(products[0].variants.nodes.len(), 2);
    assert_eq!(products[0].variants.nodes[1].id, id("ProductVariant", 2));
    assert_eq!(f.requests()[1].2["variables"]["after"], "child");
}

#![allow(clippy::unwrap_used, clippy::needless_pass_by_value)]
use super::*;
use crate::client::tests::{Fixture, Reply};
use crate::models::shopify::{OrdersByTagRequest, ProductStatus, ShopifyId};
use crate::{client::ClientError, config::ShopifyConfig};
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
        "inventoryItem":{"id":"gid://shopify/InventoryItem/1","tracked":true,"inventoryLevel":null}})
}
fn product(n: u64, variants: Value) -> Value {
    json!({"id":format!("gid://shopify/Product/{n}"),"title":"Shirt","description":"Cotton","status":"ACTIVE","tags":[],"featuredImage":null,"variants":variants})
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

#[tokio::test]
async fn catalog_collects_all_parents_and_outer_pages_with_bounded_variants() {
    let f = Fixture::new(vec![
        Reply::json(json!({"data":{"products":page(vec![
            product(1,page(vec![variant(1);250],None)),
            product(2,page(vec![variant(2)],None))],Some("p1"))}})),
        Reply::json(json!({"data":{"products":page(vec![product(3,page(vec![],None))],None)}})),
    ])
    .await;
    let result = list_products(
        &client(&f),
        &ListProductsRequest {
            location_id: id("Location", 1),
            statuses: vec![ProductStatus::Active, ProductStatus::Draft],
        },
    )
    .await
    .unwrap();
    assert_eq!(
        result.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
        vec![id("Product", 1), id("Product", 2), id("Product", 3)]
    );
    assert_eq!(result[0].variants.nodes.len(), 250);
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
        json!({"locationId":id("Location",1),"search":"status:active,draft","productsAfter":null})
    );
    assert_eq!(requests[1].2["variables"]["productsAfter"], "p1");
    let query = requests[0].2["query"].as_str().unwrap();
    assert!(query.contains("products(first: 250"));
    assert!(query.contains("variants(first: 250)"));
}

#[tokio::test]
async fn catalog_keeps_first_250_variants_without_child_cursor_or_overflow_error() {
    let f=Fixture::new(vec![Reply::json(json!({"data":{"products":page(vec![product(1,page(vec![variant(1);250],Some("unused")))],None)}}))]).await;
    let products = list_products(
        &client(&f),
        &ListProductsRequest {
            location_id: id("Location", 1),
            statuses: vec![],
        },
    )
    .await
    .unwrap();
    assert_eq!(products[0].variants.nodes.len(), 250);
    assert!(products[0].variants.page_info.has_next_page);
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
    let orders = orders_by_payment_id(
        &client(&f),
        &OrdersByPaymentRequest {
            payment_id: "a\"b\\c:(d)".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(orders.len(), 2);
    assert_eq!(
        f.requests()[0].2["variables"]["search"],
        "payment_id:\"a\\\"b\\\\c\\:\\(d\\)\""
    );
}

fn child_order(n: u64, lines: Value) -> Value {
    json!({"data":{"order":{"id":format!("gid://shopify/Order/{n}"),"lineItems":lines}}})
}
#[tokio::test]
async fn sales_collects_all_parents_and_child_pages_and_keeps_offset_range() {
    let f=Fixture::new(vec![
  Reply::json(json!({"data":{"orders":page(vec![sales(1,page(vec![],None)),sales(2,page(vec![],None))],Some("o1"))}})),
  Reply::json(json!({"data":{"orders":page(vec![sales(3,page(vec![],None))],None)}})),
  Reply::json(child_order(1,page((1..=250).map(line).collect(),Some("a")))),
  Reply::json(child_order(1,page((251..=500).map(line).collect(),Some("b")))),
  Reply::json(child_order(1,page(vec![line(501)],None))),
  Reply::json(child_order(2,page(vec![],None))),
  Reply::json(child_order(3,page(vec![line(4)],None))),
 ]).await;
    let result = sales_in_range(
        &client(&f),
        &SalesRangeRequest {
            start: "2026-09-01T00:00:00+07:00".parse().unwrap(),
            end: "2026-10-01T00:00:00+07:00".parse().unwrap(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        result.iter().map(|o| o.id.clone()).collect::<Vec<_>>(),
        vec![id("Order", 1), id("Order", 2), id("Order", 3)]
    );
    assert_eq!(result[0].line_items.nodes.len(), 501);
    assert_eq!(result[0].line_items.nodes[250].id, "line-251");
    assert_eq!(result[0].line_items.nodes[500].id, "line-501");
    assert!(!result[0].line_items.page_info.has_next_page);
    assert!(result[0].test);
    assert!(result[0].cancelled_at.is_some());
    assert_eq!(result[0].line_items.nodes[0].current_quantity, 1);
    assert_eq!(
        result[0].line_items.nodes[0]
            .original_unit_price_set
            .shop_money
            .currency_code,
        "THB"
    );
    let requests = f.requests();
    assert_eq!(requests.len(), 7);
    assert_eq!(
        requests[0].2["variables"],
        json!({"search":"created_at:>='2026-09-01T00:00:00+07:00' created_at:<'2026-10-01T00:00:00+07:00'","ordersAfter":null})
    );
    assert_eq!(requests[1].2["variables"]["ordersAfter"], "o1");
    for (index, parent, after) in [
        (2, 1, Value::Null),
        (3, 1, json!("a")),
        (4, 1, json!("b")),
        (5, 2, Value::Null),
        (6, 3, Value::Null),
    ] {
        assert_eq!(
            requests[index].2["variables"],
            json!({"id":id("Order",parent),"after":after})
        );
    }
    assert!(
        requests[0].2["query"]
            .as_str()
            .unwrap()
            .find("lineItems")
            .is_none()
    );
    assert!(
        requests[2].2["query"]
            .as_str()
            .unwrap()
            .contains("lineItems(first: 250, after: $after)")
    );
}
#[tokio::test]
async fn inventory_filters_only_inactive_positive_tracked_stock_across_pages() {
    let f=Fixture::new(vec![
        Reply::json(json!({"data":{"location":{"inventoryLevels":page(vec![
            inventory(1,false,1,true),inventory(2,true,1,true),inventory(3,false,0,true),
            inventory(4,false,-1,true),inventory(5,false,1,false)],Some("next"))}}})),
        Reply::json(json!({"data":{"location":{"inventoryLevels":page(vec![inventory(6,false,2,true)],None)}}})),
    ]).await;
    let result = find_inventory_to_activate(
        &client(&f),
        &InventoryDiscoveryRequest {
            location_id: id("Location", 1),
        },
    )
    .await
    .unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[1].item.id, id("InventoryItem", 6));
    assert_eq!(f.requests().len(), 2);
}
#[tokio::test]
async fn activation_preserves_quantities_and_idempotency_key_on_throttling() {
    let f=Fixture::new(vec![Reply::status(429),Reply::json(json!({"data":{"inventoryActivate":{
        "inventoryLevel":{"id":"level-1","isActive":true,"quantities":[{"name":"available","quantity":1},{"name":"on_hand","quantity":1}]},"userErrors":[]
    }}}))]).await;
    let key = Uuid::parse_str("79c99261-13df-45c7-818c-f24b111a75c9").unwrap();
    let level = activate_inventory(
        &client(&f),
        &ActivateInventoryRequest {
            inventory_item_id: id("InventoryItem", 1),
            location_id: id("Location", 1),
            idempotency_key: key,
        },
    )
    .await
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
async fn replacing_tags_can_clear_them_and_returns_user_errors() {
    let f = Fixture::new(vec![Reply::json(json!({"data":{"orderUpdate":{
        "order":{"id":"gid://shopify/Order/1","name":"#1","tags":[]},"userErrors":[]
    }}}))])
    .await;
    let updated = replace_order_tags(
        &client(&f),
        &ReplaceOrderTagsRequest {
            order_id: id("Order", 1),
            tags: vec![],
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.tags, Vec::<String>::new());
    assert_eq!(f.requests()[0].2["variables"]["input"]["tags"], json!([]));
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"orderUpdate":{"order":null,
        "userErrors":[{"field":["tags"],"message":"not permitted"}]}}}),
    )])
    .await;
    assert!(matches!(
        replace_order_tags(
            &client(&f),
            &ReplaceOrderTagsRequest {
                order_id: id("Order", 1),
                tags: vec![]
            }
        )
        .await,
        Err(ClientError::Mutation { .. })
    ));
}

#[tokio::test]
async fn pagination_rejects_repeated_missing_and_cyclic_cursors() {
    for cursors in [
        vec![Some("a"), Some("a")],
        vec![Some("a"), Some("b"), Some("a")],
        vec![None],
    ] {
        let replies = cursors
            .iter()
            .map(|cursor| {
                let mut connection = page(vec![], *cursor);
                connection["pageInfo"]["hasNextPage"] = json!(true);
                Reply::json(json!({"data":{"orders":connection}}))
            })
            .collect();
        let f = Fixture::new(replies).await;
        assert!(matches!(
            orders_by_payment_id(
                &client(&f),
                &OrdersByPaymentRequest {
                    payment_id: "ref".into()
                }
            )
            .await,
            Err(ClientError::Protocol(_))
        ));
        assert_eq!(f.requests().len(), cursors.len());
    }
}
#[tokio::test]
async fn empty_results_are_not_errors_and_invalid_inputs_never_call_shopify() {
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"orders":page(vec![],None)}}),
    )])
    .await;
    assert_eq!(
        orders_by_payment_id(
            &client(&f),
            &OrdersByPaymentRequest {
                payment_id: "ref".into()
            }
        )
        .await
        .unwrap()
        .len(),
        0
    );
    let f = Fixture::new(vec![]).await;
    assert!(
        list_products(
            &client(&f),
            &ListProductsRequest {
                location_id: id("Order", 1),
                statuses: vec![]
            }
        )
        .await
        .is_err()
    );
    assert!(
        orders_by_payment_id(
            &client(&f),
            &OrdersByPaymentRequest {
                payment_id: String::new()
            }
        )
        .await
        .is_err()
    );
    assert!(
        sales_in_range(
            &client(&f),
            &SalesRangeRequest {
                start: "2026-10-01T00:00:00Z".parse().unwrap(),
                end: "2026-10-01T00:00:00Z".parse().unwrap()
            }
        )
        .await
        .is_err()
    );
    assert_eq!(f.requests().len(), 0);
}
#[tokio::test]
async fn activation_user_errors_do_not_trigger_a_replay_or_quantity_reset() {
    let f=Fixture::new(vec![Reply::json(json!({"data":{"inventoryActivate":{"inventoryLevel":null,"userErrors":[{"field":null,"message":"not permitted"}]}}}))]).await;
    let result = activate_inventory(
        &client(&f),
        &ActivateInventoryRequest {
            inventory_item_id: id("InventoryItem", 1),
            location_id: id("Location", 1),
            idempotency_key: Uuid::new_v4(),
        },
    )
    .await;
    assert!(matches!(result, Err(ClientError::Mutation { .. })));
    assert_eq!(f.requests().len(), 1);
}
#[tokio::test]
async fn missing_on_hand_is_a_protocol_failure_and_missing_location_is_an_error() {
    let mut level = inventory(1, false, 1, true);
    level["quantities"] = json!([{"name":"available","quantity":1}]);
    for location in [
        json!({"inventoryLevels":page(vec![level],None)}),
        Value::Null,
    ] {
        let f = Fixture::new(vec![Reply::json(json!({"data":{"location":location}}))]).await;
        assert!(
            find_inventory_to_activate(
                &client(&f),
                &InventoryDiscoveryRequest {
                    location_id: id("Location", 1)
                }
            )
            .await
            .is_err()
        );
    }
}

fn tagged_order(n: u64, lines: Vec<Value>) -> Value {
    json!({"id":format!("gid://shopify/Order/{n}"),"number":n,"tags":["shipping"],"email":null,"phone":null,
        "currentTotalPriceSet":{"shopMoney":money()},"lineItems":{"nodes":lines}})
}
fn tagged_line() -> Value {
    json!({"title":"Shirt","variantTitle":"Blue","sku":"SHIRT-BLUE","quantity":3,
        "originalUnitPriceSet":{"shopMoney":money()}})
}
#[tokio::test]
async fn orders_by_tag_collects_order_pages_and_preserves_order_and_currency() {
    let f = Fixture::new(vec![
        Reply::json(json!({"data":{"orders":page(vec![tagged_order(3,vec![tagged_line()]), tagged_order(2,vec![])],Some("next"))}})),
        Reply::json(json!({"data":{"orders":page(vec![tagged_order(1,vec![])],None)}})),
        Reply::json(child_order(3,page(vec![tagged_line()],None))),
        Reply::json(child_order(2,page(vec![],None))),
        Reply::json(child_order(1,page(vec![],None))),
    ]).await;
    let orders = orders_by_tag(
        &client(&f),
        &OrdersByTagRequest {
            tag: "shipping".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        orders.iter().map(|o| o.number).collect::<Vec<_>>(),
        vec![3, 2, 1]
    );
    assert_eq!(
        orders[0].current_total_price_set.shop_money.amount,
        "123.4500"
    );
    assert_eq!(
        orders[0].current_total_price_set.shop_money.currency_code,
        "THB"
    );
    assert_eq!(orders[0].tags, vec!["shipping"]);
    let line = &orders[0].line_items.nodes[0];
    assert_eq!(line.sku.as_deref(), Some("SHIRT-BLUE"));
    assert_eq!(line.variant_title.as_deref(), Some("Blue"));
    assert_eq!(line.quantity, 3);
    assert_eq!(line.original_unit_price_set.shop_money.amount, "123.4500");
    assert_eq!(line.original_unit_price_set.shop_money.currency_code, "THB");
    let requests = f.requests();
    assert_eq!(requests.len(), 5);
    assert_eq!(requests[0].2["operationName"], "OrdersByTag");
    assert_eq!(
        requests[0].2["variables"],
        json!({"search":"tag:\"shipping\"","after":null})
    );
    assert_eq!(
        requests[1].2["variables"],
        json!({"search":"tag:\"shipping\"","after":"next"})
    );
    assert_eq!(requests[0].1["x-shopify-access-token"], "private-token");
    let query = requests[0].2["query"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        query.contains("first: 250 after: $after query: $search sortKey: CREATED_AT reverse: true")
    );
    assert!(!query.contains("lineItems"));
}
#[tokio::test]
async fn orders_by_tag_escapes_one_literal_and_rejects_blank_input_without_requests() {
    for (tag, search) in [
        ("ready to ship", r#"tag:"ready to ship""#),
        (r#"a"b\c:(d)"#, r#"tag:"a\"b\\c\:\(d\)""#),
    ] {
        let f = Fixture::new(vec![Reply::json(
            json!({"data":{"orders":page(vec![],None)}}),
        )])
        .await;
        assert!(
            orders_by_tag(&client(&f), &OrdersByTagRequest { tag: tag.into() })
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(f.requests()[0].2["variables"]["search"], search);
    }
    let f = Fixture::new(vec![]).await;
    for tag in ["", " \t\n"] {
        assert!(matches!(
            orders_by_tag(&client(&f), &OrdersByTagRequest { tag: tag.into() }).await,
            Err(ClientError::InvalidInput(_))
        ));
    }
    assert_eq!(f.requests().len(), 0);
}
#[tokio::test]
async fn orders_by_tag_rejects_missing_repeated_and_cyclic_continuations() {
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
                Reply::json(json!({"data":{"orders":connection}}))
            })
            .collect();
        let f = Fixture::new(replies).await;
        assert!(matches!(
            orders_by_tag(
                &client(&f),
                &OrdersByTagRequest {
                    tag: "shipping".into()
                }
            )
            .await,
            Err(ClientError::Protocol(_))
        ));
        assert_eq!(f.requests().len(), cursors.len());
    }
}
#[tokio::test]
async fn orders_by_tag_propagates_http_and_graphql_errors_without_partial_results() {
    for graphql in [false, true] {
        let failure = if graphql {
            Reply::json(
                json!({"data":null,"errors":[{"message":"denied","extensions":{"code":"ACCESS_DENIED"}}]}),
            )
        } else {
            Reply::status(503)
        };
        let f = Fixture::new(vec![
            Reply::json(json!({"data":{"orders":page(vec![tagged_order(2,vec![])],Some("next"))}})),
            failure,
        ])
        .await;
        let result = orders_by_tag(
            &client(&f),
            &OrdersByTagRequest {
                tag: "shipping".into(),
            },
        )
        .await;
        if graphql {
            assert!(matches!(result, Err(ClientError::Graphql { .. })));
        } else {
            assert!(matches!(result, Err(ClientError::Http { .. })));
        }
        assert_eq!(f.requests().len(), 2);
    }
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
#[tokio::test]
async fn tagged_orders_collect_more_than_250_lines_and_reset_child_cursors() {
    let f=Fixture::new(vec![
  Reply::json(json!({"data":{"orders":page(vec![tagged_order(1,vec![]),tagged_order(2,vec![])],None)}})),
  Reply::json(child_order(1,page(tagged_lines(1,250),Some("a")))),
  Reply::json(child_order(1,page(tagged_lines(251,250),Some("b")))),
  Reply::json(child_order(1,page(tagged_lines(501,1),None))),
  Reply::json(child_order(2,page(vec![],None))),
 ]).await;
    let orders = orders_by_tag(
        &client(&f),
        &OrdersByTagRequest {
            tag: "shipping".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(orders[0].line_items.nodes.len(), 501);
    assert_eq!(orders[0].line_items.nodes[250].title, "line-251");
    assert_eq!(orders[0].line_items.nodes[500].title, "line-501");
    assert_eq!(
        orders[0].line_items.nodes[250]
            .original_unit_price_set
            .shop_money
            .currency_code,
        "THB"
    );
    assert_eq!(orders[1].line_items.nodes.len(), 0);
    let requests = f.requests();
    assert_eq!(requests.len(), 5);
    assert_eq!(requests[2].2["variables"]["after"], "a");
    assert_eq!(requests[3].2["variables"]["after"], "b");
    assert_eq!(
        requests[4].2["variables"],
        json!({"id":id("Order",2),"after":null})
    );
}
#[tokio::test]
async fn child_order_failures_never_return_partial_success() {
    for child in [
        json!({"data":{"order":null}}),
        child_order(99, page(vec![], None)),
        child_order(
            1,
            json!({"nodes":[],"pageInfo":{"hasNextPage":true,"endCursor":null}}),
        ),
        child_order(
            1,
            json!({"nodes":[],"pageInfo":{"hasNextPage":true,"endCursor":""}}),
        ),
        child_order(1, page(vec![], Some("a"))),
        json!({"data":null,"errors":[{"message":"denied"}]}),
    ] {
        for tagged in [false, true] {
            let root = if tagged {
                tagged_order(1, vec![])
            } else {
                sales(1, page(vec![], None))
            };
            let lines = if tagged {
                vec![tagged_line()]
            } else {
                vec![line(1)]
            };
            let f = Fixture::new(vec![
                Reply::json(json!({"data":{"orders":page(vec![root],None)}})),
                Reply::json(child_order(1, page(lines, Some("a")))),
                Reply::json(child.clone()),
            ])
            .await;
            let failed = if tagged {
                orders_by_tag(
                    &client(&f),
                    &OrdersByTagRequest {
                        tag: "shipping".into(),
                    },
                )
                .await
                .is_err()
            } else {
                sales_in_range(
                    &client(&f),
                    &SalesRangeRequest {
                        start: "2026-01-01T00:00:00Z".parse().unwrap(),
                        end: "2026-02-01T00:00:00Z".parse().unwrap(),
                    },
                )
                .await
                .is_err()
            };
            assert!(failed);
            assert_eq!(f.requests().len(), 3);
        }
    }
}

#[tokio::test]
async fn later_child_http_failure_and_cursor_cycle_never_return_partial_orders() {
    for tagged in [false, true] {
        for cycle in [false, true] {
            let root = if tagged {
                tagged_order(1, vec![])
            } else {
                sales(1, page(vec![], None))
            };
            let mut replies = vec![
                Reply::json(json!({"data":{"orders":page(vec![root],None)}})),
                Reply::json(child_order(1, page(vec![], Some("a")))),
            ];
            if cycle {
                replies.push(Reply::json(child_order(1, page(vec![], Some("b")))));
                replies.push(Reply::json(child_order(1, page(vec![], Some("a")))));
            } else {
                replies.push(Reply::status(503));
            }
            let f = Fixture::new(replies).await;
            let error = if tagged {
                orders_by_tag(
                    &client(&f),
                    &OrdersByTagRequest {
                        tag: "shipping".into(),
                    },
                )
                .await
                .err()
                .unwrap()
            } else {
                sales_in_range(
                    &client(&f),
                    &SalesRangeRequest {
                        start: "2026-01-01T00:00:00Z".parse().unwrap(),
                        end: "2026-02-01T00:00:00Z".parse().unwrap(),
                    },
                )
                .await
                .err()
                .unwrap()
            };
            if cycle {
                assert!(matches!(
                    error,
                    ClientError::Protocol("pagination cursor did not progress")
                ));
            } else {
                assert!(matches!(error, ClientError::Http { status: 503, .. }));
            }
            assert_eq!(f.requests().len(), if cycle { 4 } else { 3 });
        }
    }
}

#![allow(clippy::unwrap_used, clippy::needless_pass_by_value)]
use super::*;
use crate::client::tests::{Fixture, Reply};
use crate::models::shopify::{ProductStatus, ShopifyId};
use serde_json::{Value, json};
use uuid::Uuid;
fn id(kind: &str, n: u64) -> ShopifyId {
    ShopifyId::new(format!("gid://shopify/{kind}/{n}")).unwrap()
}
fn client(f: &Fixture) -> ShopifyClient {
    ShopifyClient::with_endpoint(
        &ShopifyConfig {
            admin_api_url: f.url.to_string(),
            access_token: Some("private-token".into()),
        },
        f.url.clone(),
    )
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
        "currentTotalPriceSet":{"shopMoney":money(),"presentmentMoney":money()},"transactions":[]})
}
fn line(n: u64) -> Value {
    json!({"id":format!("line-{n}"),"title":"Shirt","variantTitle":null,"sku":null,"quantity":3,"currentQuantity":1,
        "product":null,"variant":null,"originalUnitPriceSet":{"shopMoney":money(),"presentmentMoney":money()}})
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
async fn collects_every_product_and_nested_variant_page() {
    let f=Fixture::new(vec![
        Reply::json(json!({"data":{"products":page(vec![product(1,page(vec![variant(1)],Some("v1")))],Some("p1"))}})),
        Reply::json(json!({"data":{"products":page(vec![product(1,page(vec![variant(2)],None))],Some("p1"))}})),
        Reply::json(json!({"data":{"products":page(vec![product(2,page(vec![variant(3)],None))],None)}})),
    ]).await;
    let products = client(&f)
        .list_products(&ListProductsRequest {
            location_id: id("Location", 1),
            statuses: vec![ProductStatus::Active, ProductStatus::Draft],
        })
        .await
        .unwrap();
    assert_eq!(products.len(), 2);
    assert_eq!(products[0].variants.nodes.len(), 2);
    assert_eq!(
        products[0].variants.nodes[0]
            .contextual_pricing
            .price
            .currency_code,
        "THB"
    );
    let requests = f.requests();
    assert_eq!(requests[0].2["variables"]["search"], "status:active,draft");
    assert_eq!(requests[1].2["variables"]["variantsAfter"], "v1");
    assert_eq!(requests[2].2["variables"]["productsAfter"], "p1");
    assert!(requests[2].2["variables"]["variantsAfter"].is_null());
    assert_eq!(requests[0].1["x-shopify-access-token"], "private-token");
}
#[tokio::test]
async fn payment_lookup_escapes_literal_and_collects_all_matches() {
    let f = Fixture::new(vec![
        Reply::json(json!({"data":{"orders":page(vec![order(1)],Some("a"))}})),
        Reply::json(json!({"data":{"orders":page(vec![order(2)],None)}})),
    ])
    .await;
    let orders = client(&f)
        .orders_by_payment_id(&OrdersByPaymentRequest {
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
async fn sales_collects_nested_pages_and_retains_all_statuses_and_test_orders() {
    let f=Fixture::new(vec![
        Reply::json(json!({"data":{"orders":page(vec![sales(1,page(vec![line(1)],Some("l1")))],Some("o1"))}})),
        Reply::json(json!({"data":{"orders":page(vec![sales(1,page(vec![line(2)],None))],Some("o1"))}})),
        Reply::json(json!({"data":{"orders":page(vec![sales(2,page(vec![line(3)],None))],None)}})),
    ]).await;
    let result = client(&f)
        .sales_in_range(&SalesRangeRequest {
            start: "2026-09-01T00:00:00+07:00".parse().unwrap(),
            end: "2026-10-01T00:00:00+07:00".parse().unwrap(),
        })
        .await
        .unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].line_items.nodes.len(), 2);
    assert!(result[0].test);
    assert!(result[0].cancelled_at.is_some());
    assert_eq!(result[0].line_items.nodes[0].quantity, 3);
    assert_eq!(result[0].line_items.nodes[0].current_quantity, 1);
    assert_eq!(
        f.requests()[0].2["variables"]["search"],
        "created_at:>='2026-09-01T00:00:00+07:00' created_at:<'2026-10-01T00:00:00+07:00'"
    );
    assert_eq!(f.requests()[1].2["variables"]["lineItemsAfter"], "l1");
}
#[tokio::test]
async fn inventory_filters_only_inactive_positive_tracked_stock_across_pages() {
    let f=Fixture::new(vec![
        Reply::json(json!({"data":{"location":{"inventoryLevels":page(vec![
            inventory(1,false,1,true),inventory(2,true,1,true),inventory(3,false,0,true),
            inventory(4,false,-1,true),inventory(5,false,1,false)],Some("next"))}}})),
        Reply::json(json!({"data":{"location":{"inventoryLevels":page(vec![inventory(6,false,2,true)],None)}}})),
    ]).await;
    let result = client(&f)
        .find_inventory_to_activate(&InventoryDiscoveryRequest {
            location_id: id("Location", 1),
        })
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
    let level = client(&f)
        .activate_inventory(&ActivateInventoryRequest {
            inventory_item_id: id("InventoryItem", 1),
            location_id: id("Location", 1),
            idempotency_key: key,
        })
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
    let updated = client(&f)
        .replace_order_tags(&ReplaceOrderTagsRequest {
            order_id: id("Order", 1),
            tags: vec![],
        })
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
        client(&f)
            .replace_order_tags(&ReplaceOrderTagsRequest {
                order_id: id("Order", 1),
                tags: vec![]
            })
            .await,
        Err(ClientError::Mutation { .. })
    ));
}
#[test]
fn constructor_validates_url_and_requires_injected_token() {
    for url in [
        "http://shop.myshopify.com/admin/api/2026-10/graphql.json",
        "https://evil.test/admin/api/2026-10/graphql.json",
        "https://shop.myshopify.com/incorrect",
    ] {
        assert!(
            ShopifyClient::new(&ShopifyConfig {
                admin_api_url: url.into(),
                access_token: Some("token".into())
            })
            .is_err()
        );
    }
    assert!(
        ShopifyClient::new(&ShopifyConfig {
            admin_api_url: "https://shop.myshopify.com/admin/api/2026-10/graphql.json".into(),
            access_token: None
        })
        .is_err()
    );
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
            client(&f)
                .orders_by_payment_id(&OrdersByPaymentRequest {
                    payment_id: "ref".into()
                })
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
        client(&f)
            .orders_by_payment_id(&OrdersByPaymentRequest {
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
            .list_products(&ListProductsRequest {
                location_id: id("Order", 1),
                statuses: vec![]
            })
            .await
            .is_err()
    );
    assert!(
        client(&f)
            .orders_by_payment_id(&OrdersByPaymentRequest {
                payment_id: String::new()
            })
            .await
            .is_err()
    );
    assert!(
        client(&f)
            .sales_in_range(&SalesRangeRequest {
                start: "2026-10-01T00:00:00Z".parse().unwrap(),
                end: "2026-10-01T00:00:00Z".parse().unwrap()
            })
            .await
            .is_err()
    );
    assert_eq!(f.requests().len(), 0);
}
#[tokio::test]
async fn activation_user_errors_do_not_trigger_a_replay_or_quantity_reset() {
    let f=Fixture::new(vec![Reply::json(json!({"data":{"inventoryActivate":{"inventoryLevel":null,"userErrors":[{"field":null,"message":"not permitted"}]}}}))]).await;
    let result = client(&f)
        .activate_inventory(&ActivateInventoryRequest {
            inventory_item_id: id("InventoryItem", 1),
            location_id: id("Location", 1),
            idempotency_key: Uuid::new_v4(),
        })
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
            client(&f)
                .find_inventory_to_activate(&InventoryDiscoveryRequest {
                    location_id: id("Location", 1)
                })
                .await
                .is_err()
        );
    }
}

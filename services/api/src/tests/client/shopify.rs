#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    client::tests::{Fixture, Reply},
    shopify_models::*,
};
use serde_json::{Value, json};

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
fn page(nodes: &[Value], cursor: Option<&str>) -> Value {
    json!({"nodes": nodes, "pageInfo": {"hasNextPage": cursor.is_some(), "endCursor": cursor}})
}
fn tagged_order(n: u64) -> Value {
    json!({"id": format!("gid://shopify/Order/{n}"), "number": n, "tags": ["shipping"], "email": null, "phone": null,
    "currentTotalPriceSet": {"shopMoney": {"amount": "1.00", "currencyCode": "THB"}}})
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
async fn tagged_order_proxy_does_not_follow_cursors_or_load_lines() {
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"orders":page(&[tagged_order(1)],Some("next"))}}),
    )])
    .await;
    let result = client(&f)
        .orders_by_tag(
            &OrdersByTagRequest {
                tag: "shipping".into(),
            },
            None,
        )
        .await;
    assert!(
        result.is_ok(),
        "a single-page proxy must return the supplied page without requesting a continuation"
    );
    assert!(result.unwrap().orders.page_info.has_next_page);
    assert_eq!(f.requests().len(), 1);
}

#[tokio::test]
async fn root_proxies_forward_one_cursor_and_return_continuations() {
    let f = Fixture::new(vec![
        Reply::json(json!({"data":{"products":page(&[],Some("next"))}})),
        Reply::json(json!({"data":{"orders":page(&[],Some("next"))}})),
        Reply::json(json!({"data":{"orders":page(&[],Some("next"))}})),
    ])
    .await;
    let c = client(&f);
    assert!(
        c.products_with_variants(
            &ListProductsRequest {
                location_id: id("Location", 1),
                statuses: vec![]
            },
            Some("supplied")
        )
        .await
        .unwrap()
        .products
        .page_info
        .has_next_page
    );
    assert!(
        c.orders_by_payment(
            &OrdersByPaymentRequest {
                payment_id: "ref".into()
            },
            Some("supplied")
        )
        .await
        .unwrap()
        .orders
        .page_info
        .has_next_page
    );
    assert!(
        c.sold_variants_in_range(
            &SalesRangeRequest {
                start: "2026-01-01T00:00:00Z".parse().unwrap(),
                end: "2026-02-01T00:00:00Z".parse().unwrap()
            },
            Some("supplied")
        )
        .await
        .unwrap()
        .orders
        .page_info
        .has_next_page
    );
    let requests = f.requests();
    assert_eq!(requests.len(), 3);
    for (index, operation, cursor) in [
        (0, "ProductsWithVariants", "productsAfter"),
        (1, "OrdersByPayment", "after"),
        (2, "SoldVariantsInRange", "ordersAfter"),
    ] {
        assert_eq!(requests[index].2["operationName"], operation);
        assert_eq!(requests[index].2["variables"][cursor], "supplied");
        assert!(
            requests[index].2["query"]
                .as_str()
                .unwrap()
                .contains(operation)
        );
        assert_eq!(requests[index].1["X-Shopify-Access-Token"], "private-token");
    }
}
#[tokio::test]
async fn line_proxies_are_independent_single_pages_with_raw_parent_ids() {
    let f = Fixture::new(vec![
        Reply::json(
            json!({"data":{"order":{"id":id("Order",99),"lineItems":page(&[],Some("next"))}}}),
        ),
        Reply::json(
            json!({"data":{"order":{"id":id("Order",99),"lineItems":page(&[],Some("next"))}}}),
        ),
    ])
    .await;
    let c = client(&f);
    let input = OrderLinesRequest {
        order_id: id("Order", 1),
    };
    let sales = c
        .sales_order_lines(&input, Some("cursor"))
        .await
        .unwrap()
        .order
        .unwrap();
    let tagged = c
        .tagged_order_lines(&input, Some("cursor"))
        .await
        .unwrap()
        .order
        .unwrap();
    assert_eq!(sales.id, id("Order", 99));
    assert_eq!(tagged.id, id("Order", 99));
    assert!(sales.line_items.page_info.has_next_page);
    assert!(tagged.line_items.page_info.has_next_page);
    let requests = f.requests();
    assert_eq!(requests.len(), 2);
    for (request, operation) in requests.iter().zip(["SalesOrderLines", "TaggedOrderLines"]) {
        assert_eq!(request.2["operationName"], operation);
        assert_eq!(
            request.2["variables"],
            json!({"id":id("Order",1),"after":"cursor"})
        );
        let query = request.2["query"].as_str().unwrap();
        assert!(query.contains("lineItems(first: 250, after: $after)"));
        assert!(!query.contains("orders("));
    }
}
#[tokio::test]
async fn inventory_and_mutation_proxies_do_not_apply_service_rules() {
    let level = json!({"id":"level","isActive":true,"quantities":[],
        "item":{"id":id("InventoryItem",1),"sku":null,"tracked":false,
        "variant":{"id":id("ProductVariant",1),"title":"M","product":{"id":id("Product",1),"title":"Shirt"}}}});
    let f = Fixture::new(vec![
        Reply::json(json!({"data":{"location":{"inventoryLevels":page(&[level],Some("next"))}}})),
        Reply::json(json!({"data":{"orderUpdate":{"order":null,"userErrors":[{"field":null,"message":"denied"}]}}})),
        Reply::json(json!({"data":{"inventoryActivate":{"inventoryLevel":null,"userErrors":[{"field":null,"message":"denied"}]}}})),
    ]).await;
    let c = client(&f);
    let inventory = c
        .find_inventory_to_activate(
            &InventoryDiscoveryRequest {
                location_id: id("Location", 1),
            },
            Some("cursor"),
        )
        .await
        .unwrap()
        .location
        .unwrap()
        .inventory_levels;
    assert_eq!(inventory.nodes.len(), 1);
    assert!(inventory.nodes[0].is_active);
    assert!(!inventory.nodes[0].item.tracked);
    assert!(inventory.page_info.has_next_page);
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
            idempotency_key: uuid::Uuid::new_v4(),
        })
        .await
        .unwrap()
        .inventory_activate;
    assert!(activation.inventory_level.is_none());
    assert_eq!(activation.user_errors.len(), 1);
    let requests = f.requests();
    assert_eq!(requests.len(), 3);
    for (request, operation) in requests.iter().zip([
        "FindInventoryToActivate",
        "ReplaceOrderTags",
        "ActivateExistingInventory",
    ]) {
        assert_eq!(request.2["operationName"], operation);
        assert!(request.2["query"].as_str().unwrap().contains(operation));
    }
    assert_eq!(requests[0].2["variables"]["after"], "cursor");
}
#[tokio::test]
async fn line_request_rejects_wrong_kind_before_any_network_request() {
    let f = Fixture::new(vec![]).await;
    let c = client(&f);
    let input = OrderLinesRequest {
        order_id: id("Location", 1),
    };
    assert!(c.sales_order_lines(&input, None).await.is_err());
    assert!(c.tagged_order_lines(&input, None).await.is_err());
    assert_eq!(f.requests().len(), 0);
}

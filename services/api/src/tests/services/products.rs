#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    client::tests::{Fixture, Reply},
    config::ShopifyConfig,
};
use serde_json::{Value, json};
fn id(kind: &str, n: u64) -> ShopifyId {
    ShopifyId::new(format!("gid://shopify/{kind}/{n}")).unwrap()
}
fn page(nodes: Vec<Value>, next: Option<&str>) -> Value {
    let mut value = json!({"pageInfo":{"hasNextPage":next.is_some(),"endCursor":next}});
    value["nodes"] = Value::Array(nodes);
    value
}
fn line(qty: i64) -> Value {
    json!({"title":"Item","variantTitle":null,"sku":"GENSTSWOW01","quantity":qty,"originalUnitPriceSet":{"shopMoney":{"amount":"0.1","currencyCode":"USD"}},"product":null,"variant":null})
}
fn order(lines: Value) -> Value {
    let mut value = json!({"id":id("Order",1),"number":1,"tags":[],"email":null,"phone":null,"currentTotalPriceSet":{"shopMoney":{"amount":"1","currencyCode":"USD"}}});
    value["lineItems"] = lines;
    value
}
fn client(f: &Fixture) -> ShopifyClient {
    ShopifyClient::new(&ShopifyConfig {
        admin_api_url: f.url.to_string(),
        access_token: Some("fixture".into()),
    })
    .unwrap()
}
fn input() -> ValidatedReport {
    ValidatedReport {
        source: ReportSource::OrderTag("a:b".into()),
        page: PageRequest {
            first: 1,
            after: Some("incoming".into()),
        },
    }
}
#[tokio::test]
async fn tagged_report_completes_children_but_returns_only_selected_parent_page() {
    let f=Fixture::new(vec![Reply::json(json!({"data":{"orders":page(vec![order(page(vec![line(2)],Some("child")))],Some("parent"))}})),Reply::json(json!({"data":{"order":{"id":id("Order",1),"lineItems":page(vec![line(3)],None)}}}))]).await;
    let result = product_report(&client(&f), input()).await.unwrap();
    assert_eq!(result.products.len(), 1);
    assert_eq!(result.products[0].quantity, 5);
    assert_eq!(result.products[0].total_selling_price.to_string(), "0.5");
    assert_eq!(result.page_info.end_cursor.as_deref(), Some("parent"));
    let requests = f.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].2["variables"]["first"], 1);
    assert_eq!(requests[0].2["variables"]["after"], "incoming");
    assert_eq!(requests[0].2["variables"]["search"], r#"tag:"a\:b""#);
    assert_eq!(requests[1].2["variables"]["after"], "child");
    assert_eq!(requests[1].2["variables"]["first"], 100);
}
#[tokio::test]
async fn completed_nested_lines_do_not_require_another_query_and_missing_skus_stay_separate() {
    let mut missing = line(1);
    missing["sku"] = Value::Null;
    let f=Fixture::new(vec![Reply::json(json!({"data":{"orders":page(vec![order(page(vec![line(1),missing.clone(),line(2),missing],None))],None)}}))]).await;
    let result = product_report(&client(&f), input()).await.unwrap();
    assert_eq!(
        result
            .products
            .iter()
            .map(|r| (r.sku.as_deref(), r.quantity))
            .collect::<Vec<_>>(),
        vec![(Some("GENSTSWOW01"), 3), (None, 1), (None, 1)]
    );
    assert_eq!(f.requests().len(), 1);
}
#[tokio::test]
async fn child_protocol_or_upstream_failures_do_not_return_partial_reports() {
    for child in [
        json!({"data":{"order":null}}),
        json!({"data":{"order":{"id":id("Order",99),"lineItems":page(vec![],None)}}}),
        json!({"data":{"order":{"id":id("Order",1),"lineItems":page(vec![],Some("child"))}}}),
        json!({"data":null,"errors":[{"message":"private"}]}),
    ] {
        let f=Fixture::new(vec![Reply::json(json!({"data":{"orders":page(vec![order(page(vec![line(2)],Some("child")))],None)}})),Reply::json(child)]).await;
        let error = product_report(&client(&f), input()).await.unwrap_err();
        assert_eq!(
            axum::response::IntoResponse::into_response(error).status(),
            502
        );
        assert_eq!(f.requests().len(), 2);
    }
}
#[tokio::test]
async fn root_cursor_must_progress_even_for_empty_filtered_results() {
    for end in [Value::Null, json!(""), json!("incoming")] {
        let f = Fixture::new(vec![Reply::json(
            json!({"data":{"orders":{"nodes":[],"pageInfo":{"hasNextPage":true,"endCursor":end}}}}),
        )])
        .await;
        assert!(product_report(&client(&f), input()).await.is_err());
    }
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"orders":page(vec![],Some("next"))}}),
    )])
    .await;
    let result = product_report(&client(&f), input()).await.unwrap();
    assert_eq!(result.products.len(), 0);
    assert!(result.page_info.has_next_page);
}

#[tokio::test]
async fn inventory_child_failures_never_return_partial_rows_or_fetch_again() {
    for child in [
        json!(null),
        json!({"id":id("Product",99),"variants":page(vec![],None)}),
        json!({"id":id("Product",1),"variants":page(vec![],Some("child"))}),
    ] {
        let variant = json!({"id":id("ProductVariant",1),"title":"Large","sku":"GENSTSWOW01","image":null,"contextualPricing":{"price":{"amount":"0.1","currencyCode":"USD"}},"inventoryItem":{"id":id("InventoryItem",1),"tracked":true,"unitCost":null,"measurement":{"weight":null},"inventoryLevel":{"id":"level","isActive":true,"quantities":[{"name":"on_hand","quantity":3}]}}});
        let f=Fixture::new(vec![
            Reply::json(json!({"data":{"location":{"id":id("Location",1)}}})),
            Reply::json(json!({"data":{"products":page(vec![json!({"id":id("Product",1),"title":"Parent","description":"","status":"ACTIVE","tags":[],"featuredImage":null,"productType":"KEYCHAIN","variants":page(vec![variant],Some("child"))})],None)}})),
            Reply::json(json!({"data":{"product":child}})),
        ]).await;
        let error = product_report(
            &client(&f),
            ValidatedReport {
                source: ReportSource::Inventory(id("Location", 1)),
                page: PageRequest {
                    first: 1,
                    after: None,
                },
            },
        )
        .await
        .unwrap_err();
        let response = axum::response::IntoResponse::into_response(error);
        assert_eq!(response.status(), 502);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert!(body.get("products").is_none());
        assert_eq!(f.requests().len(), 3);
    }
}

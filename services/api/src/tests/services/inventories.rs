#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    client::tests::{Fixture, Reply},
    services::common::tests::{assert_failure, id, page, state},
};
use serde_json::{Value, json};

#[tokio::test]
async fn report_transport_timeout_maps_to_504_and_other_transport_errors_to_502() {
    use crate::client::HttpClient;
    use axum::response::IntoResponse;
    use reqwest::{Method, Url, header::HeaderMap};
    use std::time::Duration;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stalled = format!("http://{}/private", listener.local_addr().unwrap());
    for (url, status) in [(stalled.as_str(), 504), ("file:///private", 502)] {
        let error = HttpClient::new(Duration::from_millis(50))
            .unwrap()
            .request_json::<(), serde_json::Value>(
                Method::GET,
                Url::parse(url).unwrap(),
                HeaderMap::new(),
                None,
            )
            .await
            .unwrap_err();
        let client_location = error.location();
        let line = line!() + 1;
        let error = upstream(error);
        assert_eq!(client_location.file(), "services/api/src/client.rs");
        assert_eq!(error.location().file(), file!());
        assert_eq!(error.location().line(), line);
        let response = error.into_response();
        assert_eq!(response.status(), status);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            serde_json::json!({"error":"Upstream request failed"})
        );
    }
}

#[tokio::test]
async fn missing_location_identifies_resource() {
    use axum::response::IntoResponse;

    let f = Fixture::new(vec![Reply::json(json!({"data":{"location":null}}))]).await;
    let response = inventory_report(
        &state(&f),
        id("Location", 999),
        PageRequest {
            first: 50,
            after: None,
        },
    )
    .await
    .unwrap_err()
    .into_response();
    assert_eq!(response.status(), 404);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap(),
        json!({"error":"Shopify location not found"})
    );
    assert_eq!(f.requests().len(), 1);
}

fn variant(quantity: i64) -> Value {
    json!({
        "id":id("ProductVariant",1),"title":"Large","sku":"GENSTSWOW01","image":null,
        "contextualPricing":{"price":{"amount":"0.1","currencyCode":"USD"}},
        "inventoryItem":{
            "id":id("InventoryItem",1),"tracked":true,"unitCost":null,"measurement":{"weight":null},
            "inventoryLevel":{"id":"level","isActive":true,"quantities":[{"name":"on_hand","quantity":quantity}]}
        }
    })
}

fn product(n: u64, variants: Value) -> Value {
    let mut value = json!({"id":id("Product",n),"title":"Parent","description":"","status":"ACTIVE","tags":[],"featuredImage":null,"productType":"KEYCHAIN"});
    value["variants"] = variants;
    value
}

#[tokio::test]
async fn inventory_forwards_page_values_and_returns_one_root_page() {
    let f=Fixture::new(vec![
        Reply::json(json!({"data":{"location":{"id":id("Location",1)}}})),
        Reply::json(json!({"data":{"products":page(vec![product(1,page(vec![variant(3),variant(0)],None))],Some("next"))}})),
        Reply::json(json!({"data":{"products":page(vec![product(2,page(vec![variant(-2)],None))],None)}}))
    ]).await;
    let result = inventory_report(
        &state(&f),
        id("Location", 1),
        PageRequest {
            first: 50,
            after: Some("abc".into()),
        },
    )
    .await
    .unwrap();
    assert_eq!(
        result
            .products
            .iter()
            .map(|r| r.quantity)
            .collect::<Vec<_>>(),
        vec![3, 0]
    );
    let requests = f.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].2["variables"],
        json!({"first":50,"after":"abc","locationId":"gid://shopify/Location/1","search":"status:active"})
    );
    assert_eq!(
        serde_json::to_value(&result).unwrap()["page_info"],
        json!({"has_next_page":true,"end_cursor":"next"})
    );
}

#[tokio::test]
async fn nested_continuation_fails_without_a_child_lookup() {
    let f=Fixture::new(vec![Reply::json(json!({"data":{"location":{"id":id("Location",1)}}})),Reply::json(json!({"data":{"products":page(vec![product(1,page(vec![variant(3)],Some("child")))],Some("next"))}}))]).await;
    assert_failure(
        &f,
        inventory_report(
            &state(&f),
            id("Location", 1),
            PageRequest {
                first: 50,
                after: None,
            },
        )
        .await
        .unwrap_err(),
        2,
    )
    .await;
}

#[tokio::test]
async fn exactly_250_nested_nodes_are_included() {
    let f=Fixture::new(vec![Reply::json(json!({"data":{"location":{"id":id("Location",1)}}})),Reply::json(json!({"data":{"products":page(vec![product(1,page(vec![variant(1);250],None))],None)}}))]).await;
    let result = inventory_report(
        &state(&f),
        id("Location", 1),
        PageRequest {
            first: 1,
            after: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(result.products.len(), 250);
}

#[tokio::test]
async fn empty_or_filtered_inventory_pages_keep_upstream_continuation() {
    let mut inactive = variant(99);
    inactive["inventoryItem"]["inventoryLevel"]["isActive"] = json!(false);
    for nodes in [vec![], vec![product(1, page(vec![inactive], None))]] {
        let f = Fixture::new(vec![
            Reply::json(json!({"data":{"location":{"id":id("Location",1)}}})),
            Reply::json(json!({"data":{"products":page(nodes,Some("next"))}})),
        ])
        .await;
        let result = inventory_report(
            &state(&f),
            id("Location", 1),
            PageRequest {
                first: 50,
                after: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            json!({"products":[],"page_info":{"has_next_page":true,"end_cursor":"next"}})
        );
        assert_eq!(f.requests().len(), 2);
    }
}

#[tokio::test]
async fn inventory_rejects_nonprogressing_root_metadata() {
    for end in [Value::Null, json!(""), json!("abc")] {
        let f = Fixture::new(vec![
            Reply::json(json!({"data":{"location":{"id":id("Location",1)}}})),
            Reply::json(json!({"data":{"products":{"nodes":[],"pageInfo":{"hasNextPage":true,"endCursor":end}}}})),
        ]).await;
        let error = inventory_report(
            &state(&f),
            id("Location", 1),
            PageRequest {
                first: 50,
                after: Some("abc".into()),
            },
        )
        .await
        .unwrap_err();
        assert_failure(&f, error, 2).await;
    }
}

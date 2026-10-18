#![allow(clippy::unwrap_used)]
use super::*;
use crate::client::shopify::ShopifyClient;
use crate::{
    client::tests::{Fixture, Reply},
    services::common::tests::{assert_failure, id, page, state},
};
use serde_json::{Value, json};

fn line(quantity: i64) -> Value {
    json!({
        "title":"Item","variantTitle":null,"sku":"GENSTSWOW01","quantity":quantity,
        "originalUnitPriceSet":{"shopMoney":{"amount":"0.1","currencyCode":"USD"}},
        "product":null,"variant":null
    })
}

fn order(n: u64, lines: Value) -> Value {
    let mut value = json!({
        "id":id("Order",n),"number":n,"tags":[],"email":null,"phone":null,
        "currentTotalPriceSet":{"shopMoney":{"amount":"1","currencyCode":"USD"}}
    });
    value["lineItems"] = lines;
    value
}

#[tokio::test]
async fn full_report_merges_matching_skus_across_all_order_pages() {
    let f = Fixture::new(vec![
        Reply::json(
            json!({"data":{"orders":page(vec![order(1,page(vec![line(2)],None))],Some("next"))}}),
        ),
        Reply::json(json!({"data":{"orders":page(vec![order(2,page(vec![line(3)],None))],None)}})),
    ])
    .await;
    let result = order_report(&state(&f), "a:b".into()).await.unwrap();
    assert_eq!(result.products.len(), 1);
    assert_eq!(result.products[0].quantity, 5);
    assert_eq!(result.products[0].total_selling_price.to_string(), "0.5");
    assert_eq!(
        serde_json::to_value(&result)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        1
    );
    let requests = f.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].2["variables"],
        json!({"search":r#"tag:"a\:b""#,"first":250,"after":null})
    );
    assert_eq!(requests[1].2["variables"]["after"], "next");
    assert!(requests.iter().all(|r| r.2["variables"]["first"] == 250));
}

#[tokio::test]
async fn missing_skus_stay_separate_in_first_seen_order_across_pages() {
    let mut missing = line(1);
    missing["sku"] = Value::Null;
    let f=Fixture::new(vec![
        Reply::json(json!({"data":{"orders":page(vec![order(1,page(vec![missing.clone(),line(2)],None))],Some("next"))}})),
        Reply::json(json!({"data":{"orders":page(vec![order(2,page(vec![line(3),missing],None))],None)}}))
    ]).await;
    let result = order_report(&state(&f), "a:b".into()).await.unwrap();
    assert_eq!(
        result
            .products
            .iter()
            .map(|r| (r.sku.as_deref(), r.quantity))
            .collect::<Vec<_>>(),
        vec![(None, 1), (Some("GENSTSWOW01"), 5), (None, 1)]
    );
}

#[tokio::test]
async fn later_upstream_failures_never_return_partial_reports() {
    for failure in [
        Reply::status(503),
        Reply::json(json!({"data":null,"errors":[{"message":"private"}]})),
        Reply {
            status: 200,
            body: "{".into(),
            headers: vec![],
        },
    ] {
        let f=Fixture::new(vec![Reply::json(json!({"data":{"orders":page(vec![order(1,page(vec![line(2)],None))],Some("next"))}})),failure]).await;
        let error = order_report(&state(&f), "a:b".into()).await.unwrap_err();
        assert_eq!(
            error.location().file(),
            "services/api/src/services/orders.rs"
        );
        assert_failure(&f, error, 2).await;
    }
}

#[tokio::test]
async fn nested_continuation_fails_without_a_child_lookup() {
    let f=Fixture::new(vec![Reply::json(json!({"data":{"orders":page(vec![order(1,page(vec![line(2)],Some("child")))],Some("next"))}}))]).await;
    assert_failure(
        &f,
        order_report(&state(&f), "a:b".into()).await.unwrap_err(),
        1,
    )
    .await;
}

#[tokio::test]
async fn exactly_250_nested_nodes_are_included() {
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"orders":page(vec![order(1,page(vec![line(1);250],None))],None)}}),
    )])
    .await;
    let result = order_report(&state(&f), "a:b".into()).await.unwrap();
    assert_eq!(result.products[0].quantity, 250);
    assert_eq!(result.products[0].total_selling_price.to_string(), "25");
}

#[tokio::test]
async fn empty_root_pages_continue_and_missing_cursors_fail() {
    let f = Fixture::new(vec![
        Reply::json(json!({"data":{"orders":page(vec![],Some("next"))}})),
        Reply::json(json!({"data":{"orders":page(vec![],None)}})),
    ])
    .await;
    assert_eq!(
        serde_json::to_value(order_report(&state(&f), "a:b".into()).await.unwrap()).unwrap(),
        json!({"products":[]})
    );
    assert_eq!(f.requests().len(), 2);
    for end in [Value::Null, json!("")] {
        let f = Fixture::new(vec![Reply::json(
            json!({"data":{"orders":{"nodes":[],"pageInfo":{"hasNextPage":true,"endCursor":end}}}}),
        )])
        .await;
        assert_failure(
            &f,
            order_report(&state(&f), "a:b".into()).await.unwrap_err(),
            1,
        )
        .await;
    }
}

#[tokio::test]
async fn service_resolves_unavailable_shared_dependency_without_network() {
    let f = Fixture::new(vec![]).await;
    let mut state = state(&f);
    state.shopify = Ok(None);
    let error = order_report(&state, "a:b".into()).await.unwrap_err();
    assert_eq!(
        axum::response::IntoResponse::into_response(error).status(),
        503
    );
    let source = ShopifyClient::new(&crate::config::ShopifyConfig {
        admin_api_url: "invalid-url".into(),
        access_token: Some("fixture".into()),
    })
    .err()
    .unwrap();
    state.shopify = Err(std::sync::Arc::new(source));
    let error = order_report(&state, "a:b".into()).await.unwrap_err();
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(
        axum::response::IntoResponse::into_response(error).status(),
        503
    );
    assert_eq!(f.requests().len(), 0);
}

#[tokio::test]
async fn later_numeric_failures_never_return_partial_reports() {
    for kind in ["quantity-overflow", "total-overflow", "invalid-amount"] {
        let mut first = line(if kind == "quantity-overflow" {
            i64::MAX
        } else {
            1
        });
        let mut second = line(1);
        if kind == "total-overflow" {
            let amount = json!("79228162514264337593543950335");
            first["originalUnitPriceSet"]["shopMoney"]["amount"] = amount.clone();
            second["originalUnitPriceSet"]["shopMoney"]["amount"] = amount;
        } else if kind == "invalid-amount" {
            second["originalUnitPriceSet"]["shopMoney"]["amount"] = json!("private-invalid-amount");
        }
        let f = Fixture::new(vec![
            Reply::json(
                json!({"data":{"orders":page(vec![order(1,page(vec![first],None))],Some("next"))}}),
            ),
            Reply::json(
                json!({"data":{"orders":page(vec![order(2,page(vec![second],None))],None)}}),
            ),
        ])
        .await;
        assert_failure(
            &f,
            order_report(&state(&f), "a:b".into()).await.unwrap_err(),
            2,
        )
        .await;
    }
}

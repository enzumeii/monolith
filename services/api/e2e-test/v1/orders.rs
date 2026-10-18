use crate::common::{
    assert_report_auth, assert_report_error, assert_report_unavailable, authed, body_json, client,
    client_with_unreachable_shopify, client_with_upstream, client_with_upstream_script, fixture,
    shopify_page as page,
};
use axum::{Json, Router, response::IntoResponse};
use domain::dto::AdminRole;
use serde_json::{Value, json};

#[tokio::test]
async fn orders_auth_validation_and_unconfigured_shopify() {
    let fixture = fixture().await;
    let client = client().await;
    let (configured, upstream) = client_with_upstream_script(vec![], "http").await;
    assert_report_auth(&configured, "/v1/orders/report?tags=report").await;
    for role in [AdminRole::Admin, AdminRole::SuperAdmin] {
        for uri in [
            "/v1/orders/report",
            "/v1/orders/report?tags=",
            "/v1/orders/report?tags=%20",
            "/v1/orders/report?tags=report&page_size=1",
            "/v1/orders/report?tags=report&cursor=next",
            "/v1/orders/report?tags=report&unknown=x",
            "/v1/orders/report?tags=report&tags=other",
        ] {
            let response = configured
                .send(authed("GET", uri, fixture.root_id(), role))
                .await
                .unwrap();
            assert_report_error(
                response,
                400,
                "Expected a nonblank tags value without pagination",
            )
            .await;
        }
        assert_report_unavailable(
            &client,
            "/v1/orders/report?tags=report",
            fixture.root_id(),
            role,
        )
        .await;
    }
    assert!(upstream.requests().is_empty());
}

#[tokio::test]
async fn order_dependency_failures_return_safe_errors_over_tcp() {
    let fixture = fixture().await;
    let mut timeouts = tokio::task::JoinSet::new();
    for later in [false, true] {
        timeouts.spawn(assert_order_failure(fixture.root_id(), later, "timeout"));
    }
    for later in [false, true] {
        for failure in [
            "http",
            "graphql",
            "partial",
            "malformed",
            "missing",
            "shape",
            "throttled",
        ] {
            assert_order_failure(fixture.root_id(), later, failure).await;
        }
    }
    let client = client_with_unreachable_shopify().await;
    let response = client
        .send(authed(
            "GET",
            "/v1/orders/report?tags=report",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_report_error(response, 502, "Shopify upstream failure").await;
    while let Some(result) = timeouts.join_next().await {
        result.unwrap();
    }
}

async fn assert_order_failure(admin_id: i64, later: bool, failure: &'static str) {
    let prefix = if later {
        vec![json!({"orders":page(vec![json!({
            "id":"gid://shopify/Order/1","number":1,"tags":[],"email":null,"phone":null,
            "currentTotalPriceSet":{"shopMoney":{"amount":"1","currencyCode":"USD"}},
            "lineItems":page(vec![report_line(2,"0.1","USD")],None)
        })],Some("next"))})]
    } else {
        vec![]
    };
    let (client, upstream) = client_with_upstream_script(prefix, failure).await;
    let response = client
        .send(authed(
            "GET",
            "/v1/orders/report?tags=report",
            admin_id,
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_report_error(
        response,
        if failure == "timeout" { 504 } else { 502 },
        "Shopify upstream failure",
    )
    .await;
    let requests = upstream.requests();
    assert_eq!(
        requests.len(),
        usize::from(later) + if failure == "throttled" { 4 } else { 1 },
        "{failure}, later={later}"
    );
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(request["operationName"], "OrdersByTag");
        assert_eq!(
            request["variables"],
            json!({"first":250,"after":if later && index > 0 { Some("next") } else { None },"search":"tag:\"report\""})
        );
    }
}

#[tokio::test]
async fn orders_invalid_shopify_configuration_returns_503() {
    let fixture = fixture().await;
    let client = crate::common::client_with_shopify("invalid-url").await;
    assert_report_unavailable(
        &client,
        "/v1/orders/report?tags=report",
        fixture.root_id(),
        AdminRole::Admin,
    )
    .await;
}

fn report_line(qty: i64, price: &str, currency: &str) -> Value {
    json!({"title":"Item","variantTitle":"Large","sku":"GENSTSWOW01","quantity":qty,
        "originalUnitPriceSet":{"shopMoney":{"amount":price,"currencyCode":currency}},"product":{"productType":"KEYCHAIN"},"variant":{"inventoryItem":{"unitCost":null,"measurement":{"weight":null}}}})
}

async fn shopify(Json(request): Json<Value>) -> axum::response::Response {
    let v = &request["variables"];
    let response = match request["operationName"].as_str().unwrap() {
        "OrdersByTag" => {
            assert_eq!(v["first"], 250);
            assert!(v.get("linesFirst").is_none());
            assert!(
                request["query"]
                    .as_str()
                    .unwrap()
                    .contains("lineItems(first: 250)")
            );
            assert!(
                v["search"] == "tag:\"empty\""
                    || v["search"] == "tag:\"report\""
                    || v["search"] == "tag:\"too-many\""
                    || v["search"] == r#"tag:"a\:b,c d""#
            );
            if v["search"] == "tag:\"too-many\"" {
                json!({"orders":page(vec![json!({"id":"gid://shopify/Order/1","number":1,"tags":[],"email":null,"phone":null,"currentTotalPriceSet":{"shopMoney":{"amount":"1","currencyCode":"USD"}},"lineItems":page(vec![report_line(1,"0.1","USD");250],Some("more"))})],None)})
            } else if v["search"] == "tag:\"empty\"" {
                json!({"orders":page(vec![],None)})
            } else {
                let first = v["after"].is_null();
                if !first {
                    assert_eq!(v["after"], "orders-2")
                }
                let lines = if first {
                    page(
                        vec![
                            report_line(2, "0.10", "USD"),
                            report_line(3, "0.1", "USD"),
                            report_line(1, "0.2", "USD"),
                        ],
                        None,
                    )
                } else {
                    let mut deleted = report_line(1, "1", "USD");
                    deleted["sku"] = Value::Null;
                    deleted["product"] = Value::Null;
                    deleted["variant"] = Value::Null;
                    page(
                        vec![
                            report_line(1, "0.1", "USD"),
                            report_line(1, "0.1", "THB"),
                            deleted.clone(),
                            deleted,
                        ],
                        None,
                    )
                };
                json!({"orders":page(vec![json!({"id":if first {"gid://shopify/Order/1"}else{"gid://shopify/Order/2"},"number":1,"tags":[],"email":null,"phone":null,"currentTotalPriceSet":{"shopMoney":{"amount":"1","currencyCode":"USD"}},"lineItems":lines})],if first {Some("orders-2")}else{None})})
            }
        }
        other => panic!("unexpected operation {other}"),
    };
    Json(json!({"data":response})).into_response()
}

#[tokio::test]
async fn complete_order_reports_use_real_tcp() {
    let fixture = fixture().await;
    let (client, _upstream) =
        client_with_upstream(Router::new().route("/graphql.json", axum::routing::post(shopify)))
            .await;
    let response = client
        .send(authed(
            "GET",
            "/v1/orders/report?tags=report",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_json(response).await;
    let rows = body["products"].as_array().unwrap();
    assert!(body.get("page_info").is_none());
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0]["quantity"], 6);
    assert_eq!(rows[0]["total_selling_price"], 0.6);
    assert!(rows[0]["cost"].is_null());
    assert!(rows[0]["total_weight"].is_null());
    assert_eq!(rows[1]["selling_price"], 0.2);

    assert_eq!(rows[2]["selling_price_currency"], "THB");
    assert!(rows[3]["category"].is_null());
    let response = client
        .send(authed(
            "GET",
            "/v1/orders/report?tags=too-many",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 502);
    assert!(body_json(response).await.get("products").is_none());
    let response = client
        .send(authed(
            "GET",
            "/v1/orders/report?tags=a%3Ab%2Cc%20d",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(body_json(response).await["products"][0]["quantity"], 6);
    let response = client
        .send(authed(
            "GET",
            "/v1/orders/report?tags=empty",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(body_json(response).await, json!({"products":[]}));
}

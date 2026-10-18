use crate::common::{
    assert_report_auth, assert_report_error, assert_report_unavailable, authed, body_json, client,
    client_with_unreachable_shopify, client_with_upstream, client_with_upstream_script, fixture,
    shopify_page as page,
};
use axum::{Json, Router, http::StatusCode, response::IntoResponse};
use domain::dto::AdminRole;
use serde_json::{Value, json};

#[tokio::test]
async fn inventories_auth_validation_and_unconfigured_shopify() {
    let fixture = fixture().await;
    let client = client().await;
    let (configured, upstream) = client_with_upstream_script(vec![], "http").await;
    assert_report_auth(&configured, "/v1/inventories/1").await;
    for role in [AdminRole::Admin, AdminRole::SuperAdmin] {
        for uri in [
            "/v1/inventories/bad",
            "/v1/inventories/0",
            "/v1/inventories/000",
            "/v1/inventories/-1",
            "/v1/inventories/1.5",
            "/v1/inventories/1?page_size=0",
            "/v1/inventories/1?page_size=251",
            "/v1/inventories/1?page_size=-1",
            "/v1/inventories/1?page_size=abc",
            "/v1/inventories/1?page_size=65536",
            "/v1/inventories/1?page_size=1&page_size=2",
            "/v1/inventories/1?cursor=",
            "/v1/inventories/1?cursor=%20",
            "/v1/inventories/1?cursor=a&cursor=b",
            "/v1/inventories/1?unexpected=x",
        ] {
            let response = configured
                .send(authed("GET", uri, fixture.root_id(), role))
                .await
                .unwrap();
            assert_report_error(
                response,
                400,
                "Expected a location id, page_size between 1 and 250, and a nonblank cursor",
            )
            .await;
        }
        assert_report_unavailable(&client, "/v1/inventories/1", fixture.root_id(), role).await;
    }
    assert!(upstream.requests().is_empty());
}

#[tokio::test]
async fn inventory_dependency_failures_return_safe_errors_over_tcp() {
    let fixture = fixture().await;
    let mut timeouts = tokio::task::JoinSet::new();
    for products in [false, true] {
        timeouts.spawn(assert_inventory_failure(
            fixture.root_id(),
            products,
            "timeout",
        ));
    }
    for products in [false, true] {
        for failure in [
            "http",
            "graphql",
            "partial",
            "malformed",
            "missing",
            "shape",
            "throttled",
        ] {
            assert_inventory_failure(fixture.root_id(), products, failure).await;
        }
    }
    let client = client_with_unreachable_shopify().await;
    let response = client
        .send(authed(
            "GET",
            "/v1/inventories/1",
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

async fn assert_inventory_failure(admin_id: i64, products: bool, failure: &'static str) {
    let prefix = if products {
        vec![json!({"location":{"id":"gid://shopify/Location/1"}})]
    } else {
        vec![]
    };
    let (client, upstream) = client_with_upstream_script(prefix, failure).await;
    let response = client
        .send(authed(
            "GET",
            "/v1/inventories/1",
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
        usize::from(products) + if failure == "throttled" { 4 } else { 1 },
        "{failure}, products={products}"
    );
    for (index, request) in requests.iter().enumerate() {
        if products && index > 0 {
            assert_eq!(request["operationName"], "ProductsWithVariants");
            assert_eq!(
                request["variables"],
                json!({"first":50,"after":null,"locationId":"gid://shopify/Location/1","search":"status:active"})
            );
        } else {
            assert_eq!(request["operationName"], "Location");
            assert_eq!(
                request["variables"],
                json!({"id":"gid://shopify/Location/1"})
            );
        }
    }
}

#[tokio::test]
async fn inventories_invalid_shopify_configuration_returns_503() {
    let fixture = fixture().await;
    let client = crate::common::client_with_shopify("invalid-url").await;
    assert_report_unavailable(
        &client,
        "/v1/inventories/1",
        fixture.root_id(),
        AdminRole::Admin,
    )
    .await;
}

fn report_variant(quantity: Option<i64>) -> Value {
    json!({"id":"gid://shopify/ProductVariant/1","image":null,"title":"Large","sku":"GENSTSWOW01","contextualPricing":{"price":{"amount":"0.10","currencyCode":"USD"}},"inventoryItem":{
        "id":"gid://shopify/InventoryItem/1","tracked":true,"unitCost":{"amount":"0.03","currencyCode":"USD"},"measurement":{"weight":{"value":2.5,"unit":"GRAMS"}},
        "inventoryLevel":quantity.map(|q|json!({"id":"level","isActive":true,"quantities":[{"name":"on_hand","quantity":q}]}))}})
}

async fn shopify(Json(request): Json<Value>) -> axum::response::Response {
    let v = &request["variables"];
    let response = match request["operationName"].as_str().unwrap() {
        "Location" => {
            if v["id"] == "gid://shopify/Location/999" {
                json!({"location":null})
            } else {
                json!({"location":{"id":v["id"]}})
            }
        }
        "ProductsWithVariants" => {
            assert_eq!(v["search"], "status:active");
            assert_eq!(
                v["first"],
                if v["locationId"] == "gid://shopify/Location/993" {
                    7
                } else {
                    50
                }
            );
            assert!(v.get("variantsFirst").is_none());
            assert!(
                request["query"]
                    .as_str()
                    .unwrap()
                    .contains("variants(first: 250)")
            );
            match v["locationId"].as_str().unwrap() {
                "gid://shopify/Location/998" => {
                    return (
                        StatusCode::OK,
                        Json(json!({"errors":[{"message":"secret upstream payload"}]})),
                    )
                        .into_response();
                }
                "gid://shopify/Location/997" => {
                    return (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(json!({"secret":"upstream"})),
                    )
                        .into_response();
                }
                "gid://shopify/Location/996" => return (StatusCode::OK, "{").into_response(),
                "gid://shopify/Location/993" => {
                    assert_eq!(v["after"], "abc+/=");
                    json!({"products":page(vec![],None)})
                }
                "gid://shopify/Location/995" => json!({"products":page(vec![],None)}),
                "gid://shopify/Location/994" => {
                    json!({"products":page(vec![],if v["after"].is_null(){Some("empty-next")}else{None})})
                }
                _ => {
                    let first = v["after"].is_null();
                    if !first {
                        assert_eq!(v["after"], "products-2")
                    }
                    let mut inactive = report_variant(Some(99));
                    inactive["inventoryItem"]["inventoryLevel"]["isActive"] = json!(false);
                    let variants = if first {
                        page(
                            vec![
                                report_variant(Some(3)),
                                report_variant(Some(-2)),
                                report_variant(None),
                                inactive,
                            ],
                            None,
                        )
                    } else {
                        page(vec![report_variant(Some(0))], None)
                    };
                    json!({"products":page(vec![json!({"id":"gid://shopify/Product/1","title":"Item","description":"","status":"ACTIVE","tags":[],"featuredImage":null,"productType":"KEYCHAIN","variants":variants})],if first {Some("products-2")}else{None})})
                }
            }
        }
        other => panic!("unexpected operation {other}"),
    };
    Json(json!({"data":response})).into_response()
}

#[tokio::test]
async fn inventory_pages_use_real_tcp() {
    let fixture = fixture().await;
    let (client, _upstream) =
        client_with_upstream(Router::new().route("/graphql.json", axum::routing::post(shopify)))
            .await;
    for role in [AdminRole::Admin, AdminRole::SuperAdmin] {
        let response = client
            .send(authed("GET", "/v1/inventories/1", fixture.root_id(), role))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_json(response).await;
        let rows = body["products"].as_array().unwrap();
        assert_eq!(
            body["page_info"],
            json!({"has_next_page":true,"end_cursor":"products-2"})
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            json!({"category":"KEYCHAIN","collection":"STSWOW","sku":"GENSTSWOW01","title":"Item","variant":"Large","cost":0.03,"cost_currency":"USD","weight":2.5,"unit":"GRAMS","selling_price":0.1,"selling_price_currency":"USD","quantity":3,"total_selling_price":0.3,"total_weight":7.5})
        );
        assert_eq!(rows[1]["quantity"], -2);
        assert_eq!(rows[1]["total_weight"], -5);
        let response = client
            .send(authed(
                "GET",
                "/v1/inventories/1?page_size=50&cursor=products-2",
                fixture.root_id(),
                role,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_json(response).await;
        assert_eq!(body["products"].as_array().unwrap().len(), 1);
        assert_eq!(body["products"][0]["quantity"], 0);
        assert_eq!(
            body["page_info"],
            json!({"has_next_page":false,"end_cursor":null})
        );
    }
    let response = client
        .send(authed(
            "GET",
            "/v1/inventories/994",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        body_json(response).await,
        json!({"products":[],"page_info":{"has_next_page":true,"end_cursor":"empty-next"}})
    );
    for uri in [
        "/v1/inventories/994?cursor=empty-next",
        "/v1/inventories/995",
        "/v1/inventories/993?page_size=7&cursor=abc%2B%2F%3D",
    ] {
        let response = client
            .send(authed("GET", uri, fixture.root_id(), AdminRole::Admin))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            body_json(response).await,
            json!({"products":[],"page_info":{"has_next_page":false,"end_cursor":null}})
        );
    }
    for (id, status) in [(999, 404), (998, 502), (997, 502), (996, 502)] {
        let response = client
            .send(authed(
                "GET",
                &format!("/v1/inventories/{id}"),
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        let body = body_json(response).await;
        if id == 999 {
            assert_eq!(body, json!({"error":"Shopify location not found"}));
        }
        assert!(body.get("products").is_none());
        assert!(!body.to_string().contains("secret"));
    }
}

use crate::common::{authed, body_json, client, fixture};
use domain::dto::AdminRole;

#[tokio::test]
async fn product_route_auth_validation_and_unconfigured_shopify() {
    let fixture = fixture().await;
    let client = client().await;
    let response = client
        .send(
            axum::http::Request::builder()
                .uri("/v1/products?type=INVENTORY&id=1")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    let response = client
        .send(
            axum::http::Request::builder()
                .uri("/v1/products?type=INVENTORY&id=1")
                .header("authorization", "Bearer invalid")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 401);
    for role in [AdminRole::Admin, AdminRole::SuperAdmin] {
        for query in [
            "type=OTHER&id=1",
            "type=INVENTORY&id=bad",
            "type=ORDER_TAG&id=",
            "type=INVENTORY&id=1&page_size=1",
            "type=INVENTORY&id=1&page_size=0",
            "type=INVENTORY&id=1&page_size=51",
            "type=INVENTORY&id=1&page_size=-1",
            "type=INVENTORY&id=1&page_size=abc",
            "type=INVENTORY&id=1&cursor=next",
            "type=INVENTORY&id=1&cursor=%20",
        ] {
            let response = client
                .send(authed(
                    "GET",
                    &format!("/v1/products?{query}"),
                    fixture.root_id(),
                    role,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), 400);
        }
        let response = client
            .send(authed(
                "GET",
                "/v1/products?type=INVENTORY&id=1",
                fixture.root_id(),
                role,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 503);
        assert_eq!(
            body_json(response).await["error"],
            "Shopify report unavailable"
        );
    }
}

use axum::{Json, Router, http::StatusCode, response::IntoResponse};
use serde_json::{Value, json};

fn page(nodes: Vec<Value>, next: Option<&str>) -> Value {
    json!({"nodes":nodes,"pageInfo":{"hasNextPage":next.is_some(),"endCursor":next}})
}
fn report_variant(quantity: Option<i64>) -> Value {
    json!({"id":"gid://shopify/ProductVariant/1","image":null,"title":"Large","sku":"GENSTSWOW01","contextualPricing":{"price":{"amount":"0.10","currencyCode":"USD"}},"inventoryItem":{
        "id":"gid://shopify/InventoryItem/1","tracked":true,"unitCost":{"amount":"0.03","currencyCode":"USD"},"measurement":{"weight":{"value":2.5,"unit":"GRAMS"}},
        "inventoryLevel":quantity.map(|q|json!({"id":"level","isActive":true,"quantities":[{"name":"on_hand","quantity":q}]}))}})
}

fn report_line(qty: i64, price: &str, currency: &str) -> Value {
    json!({"title":"Item","variantTitle":"Large","sku":"GENSTSWOW01","quantity":qty,
        "originalUnitPriceSet":{"shopMoney":{"amount":price,"currencyCode":currency}},"product":{"productType":"KEYCHAIN"},"variant":{"inventoryItem":{"unitCost":null,"measurement":{"weight":null}}}})
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
            assert_eq!(v["first"], 1);
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
        "OrdersByTag" => {
            assert_eq!(v["first"], 1);
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
async fn product_reports_use_real_api_tcp_and_mocked_shopify() {
    let fixture = fixture().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route("/graphql.json", axum::routing::post(shopify)),
        )
        .await
        .unwrap();
    });
    struct Server(tokio::task::JoinHandle<()>);
    impl Drop for Server {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let _server = Server(server);
    let client =
        crate::common::client_with_shopify(&format!("http://{address}/graphql.json")).await;
    for role in [AdminRole::Admin, AdminRole::SuperAdmin] {
        let response = client
            .send(authed(
                "GET",
                "/v1/products?type=INVENTORY&id=1",
                fixture.root_id(),
                role,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body = body_json(response).await;
        let rows = body["products"].as_array().unwrap();
        assert!(body.get("page_info").is_none());
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[0],
            json!({"category":"KEYCHAIN","collection":"STSWOW","sku":"GENSTSWOW01","title":"Item","variant":"Large","cost":0.03,"cost_currency":"USD","weight":2.5,"unit":"GRAMS","selling_price":0.1,"selling_price_currency":"USD","quantity":3,"total_selling_price":0.3,"total_weight":7.5})
        );
        assert_eq!(rows[1]["quantity"], -2);
        assert_eq!(rows[1]["total_weight"], -5);
        assert_eq!(rows[2]["quantity"], 0);
    }
    let response = client
        .send(authed(
            "GET",
            "/v1/products?type=ORDER_TAG&id=report",
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
            "/v1/products?type=ORDER_TAG&id=too-many",
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
            "/v1/products?type=INVENTORY&id=994",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(body_json(response).await, json!({"products":[]}));
    for query in ["type=ORDER_TAG&id=empty", "type=INVENTORY&id=995"] {
        let response = client
            .send(authed(
                "GET",
                &format!("/v1/products?{query}"),
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(body_json(response).await, json!({"products":[]}));
    }
    for (id, status) in [(999, 404), (998, 502), (997, 502), (996, 502)] {
        let response = client
            .send(authed(
                "GET",
                &format!("/v1/products?type=INVENTORY&id={id}"),
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        let body = body_json(response).await;
        assert!(body.get("products").is_none());
        assert!(!body.to_string().contains("secret"));
    }
}

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
    assert_report_auth(&configured, "/v1/admin/orders/report?tags=report").await;
    for role in [AdminRole::Admin, AdminRole::SuperAdmin] {
        for uri in [
            "/v1/admin/orders/report",
            "/v1/admin/orders/report?tags=",
            "/v1/admin/orders/report?tags=%20",
            "/v1/admin/orders/report?tags=,",
            "/v1/admin/orders/report?tags=SHIPPING,",
            "/v1/admin/orders/report?tags=,SHIPPING",
            "/v1/admin/orders/report?tags=SHIPPING,%20,TEST",
            "/v1/admin/orders/report?tags=report&page_size=1",
            "/v1/admin/orders/report?tags=report&cursor=next",
            "/v1/admin/orders/report?tags=report&unknown=x",
            "/v1/admin/orders/report?tags=report&tags=other",
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
            "/v1/admin/orders/report?tags=report",
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
            "/v1/admin/orders/report?tags=report",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_report_error(response, 502, "Upstream request failed").await;
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
    let started = std::time::Instant::now();
    let response = client
        .send(authed(
            "GET",
            "/v1/admin/orders/report?tags=report",
            admin_id,
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_report_error(
        response,
        if failure == "timeout" { 504 } else { 502 },
        "Upstream request failed",
    )
    .await;
    if failure == "timeout" {
        assert!(started.elapsed() >= std::time::Duration::from_secs(3));
        assert!(started.elapsed() < std::time::Duration::from_secs(8));
    }
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
        "/v1/admin/orders/report?tags=report",
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
                    || v["search"] == r#"tag:"a\:b" AND tag:"c d""#
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
async fn comma_separated_tags_require_all_tags_across_every_order_page() {
    let fixture = fixture().await;
    let order = |quantity| {
        json!({
            "id":"gid://shopify/Order/1","number":1,"tags":["SHIPPING","TEST"],"email":null,"phone":null,
            "currentTotalPriceSet":{"shopMoney":{"amount":"1","currencyCode":"USD"}},
            "lineItems":page(vec![report_line(quantity,"0.1","USD")],None)
        })
    };
    let (client, upstream) = client_with_upstream_script(
        vec![
            json!({"orders":page(vec![order(2)],Some("next"))}),
            json!({"orders":page(vec![order(3)],None)}),
        ],
        "http",
    )
    .await;
    let response = client
        .send(authed(
            "GET",
            "/v1/admin/orders/report?tags=%20SHIPPING%20,%20TEST%20",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_json(response).await;
    assert_eq!(body["products"].as_array().unwrap().len(), 1);
    assert_eq!(body["products"][0]["quantity"], 5);
    assert_eq!(body["products"][0]["total_selling_price"], 0.5);
    let requests = upstream.requests();
    assert_eq!(requests.len(), 2);
    for (index, request) in requests.iter().enumerate() {
        assert_eq!(request["operationName"], "OrdersByTag");
        assert_eq!(
            request["variables"],
            json!({
                "first":250,
                "after":if index == 0 { None } else { Some("next") },
                "search":"tag:\"SHIPPING\" AND tag:\"TEST\""
            })
        );
    }
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
            "/v1/admin/orders/report?tags=report",
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
            "/v1/admin/orders/report?tags=too-many",
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
            "/v1/admin/orders/report?tags=a%3Ab%2Cc%20d",
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
            "/v1/admin/orders/report?tags=empty",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(body_json(response).await, json!({"products":[]}));
}

#[tokio::test]
async fn fulfillment_workflow_routes_have_real_tcp_contracts() {
    let fixture = fixture().await;
    let (client, upstream) = client_with_upstream_script(vec![
        json!({"orders":page(vec![json!({"id":"gid://shopify/Order/42","number":1234})],Some("next"))}),
        json!({"orders":page(vec![],None)}),
    ], "http").await;
    let response = client.send(authed("GET", "/v1/admin/orders?tag=SHIPPING,FASTSHIP&page_size=1&cursor=prev&fulfillment=UNFULFILLED", fixture.root_id(), AdminRole::Admin)).await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        body_json(response).await,
        json!({"orders":[{"id":42,"number":1234}],"page_info":{"has_next_page":true,"end_cursor":"next"}})
    );
    let response = client
        .send(authed(
            "POST",
            "/v1/admin/orders/shipping_label",
            fixture.root_id(),
            AdminRole::SuperAdmin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        body_json(response).await,
        json!({"fastship":[],"thaipost":[],"best":[],"teleport":[]})
    );
    let requests = upstream.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]["variables"],
        json!({"first":1,"after":"prev","search":"tag:\"SHIPPING\" AND tag:\"FASTSHIP\" AND fulfillment_status:unfulfilled"})
    );
    assert_eq!(
        requests[1]["variables"]["search"],
        "status:open AND financial_status:paid AND fulfillment_status:unfulfilled AND tag:\"PACKING\""
    );
}

fn shipping_order(id: u64, country: &str, method: &str) -> Value {
    json!({"id":format!("gid://shopify/Order/{id}"),"email":"customer@example.test","currencyCode":"THB","totalWeight":2000,
        "shippingAddress":{"firstName":"Jane","lastName":"Doe","name":"Jane Doe","address1":"1 Test St","address2":null,"city":"Test City","provinceCode":"TS","zip":"00123","country":"Test Country","countryCodeV2":country,"phone":"+66 (123) 456"},
        "shippingLines":page(vec![json!({"code":method})],None),
        "lineItems":page(vec![json!({"title":"Keychain","sku":"ABCKEY000001","quantity":3,"originalUnitPriceSet":{"shopMoney":{"amount":"30","currencyCode":"THB"}}})],None)})
}
fn tags_result(remove: bool, id: u64) -> Value {
    json!({if remove {"tagsRemove"} else {"tagsAdd"}:{"node":{"id":format!("gid://shopify/Order/{id}")},"userErrors":[]}})
}
fn fulfillable_order(next: Option<&str>) -> Value {
    json!({"order":{"id":"gid://shopify/Order/42","fulfillmentOrders":page(vec![json!({"id":"gid://shopify/FulfillmentOrder/7","status":"OPEN","assignedLocation":{"location":{"id":"gid://shopify/Location/1"}},"supportedActions":[{"action":"CREATE_FULFILLMENT"}],"lineItems":page(vec![json!({"id":"gid://shopify/FulfillmentOrderLineItem/8","remainingQuantity":2}),json!({"id":"gid://shopify/FulfillmentOrderLineItem/9","remainingQuantity":0})],None)})],next)}})
}
#[tokio::test]
async fn shipping_labels_route_preserves_reference_carriers_and_tag_order() {
    let fixture = fixture().await;
    let orders = vec![
        shipping_order(1, "US", "Standard Shipping"),
        shipping_order(2, "AU", "Tracking number"),
        shipping_order(3, "TH", "Standard Shipping"),
        shipping_order(4, "SG", "TELEPORT AIRASIA : TRACKING & FAST"),
    ];
    let mut replies = vec![json!({"orders":page(orders,None)})];
    for id in 1..=4 {
        replies.extend([tags_result(false, id), tags_result(true, id)]);
    }
    let (client, upstream) = client_with_upstream_script(replies, "http").await;
    let response = client
        .send(authed(
            "POST",
            "/v1/admin/orders/shipping_label",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_json(response).await;
    assert_eq!(body["fastship"][0]["quantity"], 3);
    assert_eq!(body["fastship"][0]["declared_value"], 150.0);
    assert_eq!(body["fastship"][0]["incoterm"], "DDU");
    assert_eq!(body["fastship"][0]["phone"], "66123456");
    assert_eq!(body["thaipost"][0]["service_id"], 3278);
    assert_eq!(body["thaipost"][0]["sender_postcode"], "10230");
    assert_eq!(body["best"][0]["weight"], 0);
    assert_eq!(body["teleport"][0]["weight"], 2.0);
    assert_eq!(
        body["teleport"][0]["is_multi_piece"],
        "Multi Piece Shippment"
    );
    let requests = upstream.requests();
    assert_eq!(requests.len(), 9);
    for (i, courier) in ["FASTSHIP", "THAI_POST", "BEST", "TELEPORT"]
        .iter()
        .enumerate()
    {
        assert_eq!(requests[1 + i * 2]["operationName"], "AddOrderTags");
        assert_eq!(
            requests[1 + i * 2]["variables"]["tags"],
            json!(["SHIPPING", courier])
        );
        assert_eq!(requests[2 + i * 2]["operationName"], "RemoveOrderTags");
        assert_eq!(requests[2 + i * 2]["variables"]["tags"], json!(["PACKING"]));
    }
}
#[tokio::test]
async fn fulfill_route_fulfills_remaining_lines_before_removing_shipping() {
    let fixture = fixture().await;
    let (client,upstream)=client_with_upstream_script(vec![fulfillable_order(None),json!({"fulfillmentCreate":{"fulfillment":{"id":"gid://shopify/Fulfillment/10"},"userErrors":[]}}),tags_result(true,42)],"http").await;
    let mut request = authed(
        "POST",
        "/v1/admin/orders/42/fulfill",
        fixture.root_id(),
        AdminRole::Admin,
    );
    request
        .headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    *request.body_mut() =
        axum::body::Body::from(r#"{"courier":"FASTSHIP","tracking_number":"LR123"}"#);
    let response = client.send(request).await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        body_json(response).await,
        json!({"success":true,"tracking_url":"https://track.fastship.co/LR123"})
    );
    let requests = upstream.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[1]["variables"]["fulfillment"],
        json!({"notifyCustomer":true,"trackingInfo":{"company":"Other","number":"LR123","url":"https://track.fastship.co/LR123"},"lineItemsByFulfillmentOrder":[{"fulfillmentOrderId":"gid://shopify/FulfillmentOrder/7","fulfillmentOrderLineItems":[{"id":"gid://shopify/FulfillmentOrderLineItem/8","quantity":2}]}]})
    );
    assert_eq!(requests[2]["variables"]["tags"], json!(["SHIPPING"]));
}

fn fulfill_request(
    id: &str,
    body: &str,
    admin: i64,
    role: AdminRole,
) -> axum::http::Request<axum::body::Body> {
    let mut request = authed(
        "POST",
        &format!("/v1/admin/orders/{id}/fulfill"),
        admin,
        role,
    );
    request
        .headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    *request.body_mut() = axum::body::Body::from(body.to_owned());
    request
}

#[tokio::test]
async fn fulfillment_workflow_validation_rejects_before_business_calls() {
    let fixture = fixture().await;
    let (client, upstream) = client_with_upstream_script(vec![], "http").await;
    let message =
        "Expected tag, a valid fulfillment, page_size between 1 and 250, and a nonblank cursor";
    for query in [
        "",
        "?tag=",
        "?tag=%20",
        "?tag=,A",
        "?tag=A,",
        "?tag=A,,B",
        "?tag=A&tag=B",
        "?tag=A&unknown=1",
        "?tag=A&fulfillment=INVALID",
        "?tag=A&page_size=x",
        "?tag=A&page_size=0",
        "?tag=A&page_size=251",
        "?tag=A&page_size=-1",
        "?tag=A&cursor=%20",
        "?tag=A&cursor=",
    ] {
        let response = client
            .send(authed(
                "GET",
                &format!("/v1/admin/orders{query}"),
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_report_error(response, 400, message).await;
    }
    let message = "Expected a positive order id, supported courier, and tracking_number of 1 to 256 characters without whitespace or control characters";
    for id in ["x", "0", "-1", "18446744073709551616"] {
        let response = client
            .send(fulfill_request(
                id,
                r#"{"courier":"BEST","tracking_number":"VP123"}"#,
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_report_error(response, 400, message).await;
    }
    for body in [
        "",
        "{",
        "{}",
        r#"{"courier":"INVALID","tracking_number":"x"}"#,
        r#"{"courier":"BEST"}"#,
        r#"{"tracking_number":"x"}"#,
        r#"{"courier":"BEST","tracking_number":2}"#,
        r#"{"courier":"BEST","tracking_number":""}"#,
        r#"{"courier":"BEST","tracking_number":"a b"}"#,
        r#"{"courier":"BEST","tracking_number":"\n"}"#,
        r#"{"courier":"BEST","tracking_number":"x","unknown":1}"#,
        r#"{"courier":"BEST","courier":"BEST","tracking_number":"x"}"#,
    ] {
        let response = client
            .send(fulfill_request(
                "42",
                body,
                fixture.root_id(),
                AdminRole::SuperAdmin,
            ))
            .await
            .unwrap();
        assert_report_error(response, 400, message).await;
    }
    let long = json!({"courier":"BEST","tracking_number":"x".repeat(257)}).to_string();
    let response = client
        .send(fulfill_request(
            "42",
            &long,
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_report_error(response, 400, message).await;
    let response = client
        .send(authed(
            "POST",
            "/v1/admin/orders/shipping_label?x=1",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_report_error(
        response,
        400,
        "Shipping labels do not accept query parameters",
    )
    .await;
    assert!(upstream.requests().is_empty());
}

#[tokio::test]
async fn fulfillment_workflow_auth_and_optional_dependency_over_tcp() {
    let fixture = fixture().await;
    let client = client().await;
    for (method, uri) in [
        ("GET", "/v1/admin/orders?tag=X"),
        ("POST", "/v1/admin/orders/shipping_label"),
        ("POST", "/v1/admin/orders/42/fulfill"),
    ] {
        for token in [None, Some("Bearer invalid")] {
            let mut request = axum::http::Request::builder().method(method).uri(uri);
            if let Some(token) = token {
                request = request.header("authorization", token);
            }
            let response = client
                .send(request.body(axum::body::Body::empty()).unwrap())
                .await
                .unwrap();
            assert_report_error(response, 401, "Invalid or expired token").await;
        }
        for role in [AdminRole::Admin, AdminRole::SuperAdmin] {
            let request = if uri.ends_with("fulfill") {
                fulfill_request(
                    "42",
                    r#"{"courier":"TELEPORT","tracking_number":"T123"}"#,
                    fixture.root_id(),
                    role,
                )
            } else {
                authed(method, uri, fixture.root_id(), role)
            };
            assert_report_error(
                client.send(request).await.unwrap(),
                503,
                "Shopify report unavailable",
            )
            .await;
        }
    }
}

fn fulfillment_prefix(stage: usize) -> Vec<Value> {
    let mut prefix = vec![];
    if stage > 0 {
        prefix.push(fulfillable_order(None));
    }
    if stage > 1 {
        prefix.push(json!({"fulfillmentCreate":{"fulfillment":{"id":"gid://shopify/Fulfillment/10"},"userErrors":[]}}));
    }
    prefix
}
fn label_prefix(stage: usize) -> Vec<Value> {
    let mut prefix = vec![];
    if stage > 0 {
        prefix.push(json!({"orders":page(vec![shipping_order(1,"US","Standard Shipping")],None)}));
    }
    if stage > 1 {
        prefix.push(tags_result(false, 1));
    }
    prefix
}

#[tokio::test]
async fn fulfillment_workflow_each_upstream_operation_maps_failures_and_stops() {
    let fixture = fixture().await;
    for flow in ["list", "labels", "fulfill"] {
        let stages = if flow == "list" { 1 } else { 3 };
        for stage in 0..stages {
            for failure in [
                "http",
                "graphql",
                "partial",
                "malformed",
                "missing",
                "shape",
                "timeout",
                "throttled",
            ] {
                let prefix = match flow {
                    "labels" => label_prefix(stage),
                    "fulfill" => fulfillment_prefix(stage),
                    _ => vec![],
                };
                let (client, upstream) = client_with_upstream_script(prefix, failure).await;
                let request = match flow {
                    "labels" => authed(
                        "POST",
                        "/v1/admin/orders/shipping_label",
                        fixture.root_id(),
                        AdminRole::Admin,
                    ),
                    "fulfill" => fulfill_request(
                        "42",
                        r#"{"courier":"THAI_POST","tracking_number":"RR123TH"}"#,
                        fixture.root_id(),
                        AdminRole::Admin,
                    ),
                    _ => authed(
                        "GET",
                        "/v1/admin/orders?tag=X",
                        fixture.root_id(),
                        AdminRole::Admin,
                    ),
                };
                assert_report_error(
                    client.send(request).await.unwrap(),
                    if failure == "timeout" { 504 } else { 502 },
                    "Upstream request failed",
                )
                .await;
                assert_eq!(
                    upstream.requests().len(),
                    stage + if failure == "throttled" { 4 } else { 1 },
                    "{flow} {stage} {failure}"
                );
            }
        }
    }
}

#[tokio::test]
async fn fulfillment_workflow_connection_failures_over_tcp() {
    let fixture = fixture().await;
    let client = client_with_unreachable_shopify().await;
    for request in [
        authed(
            "GET",
            "/v1/admin/orders?tag=X",
            fixture.root_id(),
            AdminRole::Admin,
        ),
        authed(
            "POST",
            "/v1/admin/orders/shipping_label",
            fixture.root_id(),
            AdminRole::Admin,
        ),
        fulfill_request(
            "42",
            r#"{"courier":"TELEPORT","tracking_number":"T123"}"#,
            fixture.root_id(),
            AdminRole::Admin,
        ),
    ] {
        assert_report_error(
            client.send(request).await.unwrap(),
            502,
            "Upstream request failed",
        )
        .await;
    }
}

#[tokio::test]
async fn shipping_labels_validates_all_pages_before_writes_and_handles_later_page_failure() {
    let fixture = fixture().await;
    for failure in ["http", "graphql", "malformed", "timeout"] {
        let (client, upstream) = client_with_upstream_script(
            vec![json!({"orders":page(vec![shipping_order(1,"US","USPS")],Some("next"))})],
            failure,
        )
        .await;
        assert_report_error(
            client
                .send(authed(
                    "POST",
                    "/v1/admin/orders/shipping_label",
                    fixture.root_id(),
                    AdminRole::Admin,
                ))
                .await
                .unwrap(),
            if failure == "timeout" { 504 } else { 502 },
            "Upstream request failed",
        )
        .await;
        let requests = upstream.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1]["variables"]["after"], "next");
    }
    let mut bad = shipping_order(2, "JP", "Standard Shipping");
    bad["shippingAddress"] = Value::Null;
    let (client, upstream) = client_with_upstream_script(
        vec![json!({"orders":page(vec![shipping_order(1,"US","USPS"),bad],None)})],
        "http",
    )
    .await;
    assert_report_error(
        client
            .send(authed(
                "POST",
                "/v1/admin/orders/shipping_label",
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap(),
        502,
        "Order shipping address missing",
    )
    .await;
    assert_eq!(upstream.requests().len(), 1);
}

#[tokio::test]
async fn fulfillment_workflow_domain_outcomes_and_mutation_rejections() {
    let fixture = fixture().await;
    for (reply, status, message) in [
        (json!({"order":null}), 404, "Shopify order not found"),
        (
            json!({"order":{"id":"gid://shopify/Order/42","fulfillmentOrders":page(vec![],None)}}),
            409,
            "Order has no remaining fulfillable items",
        ),
    ] {
        let (client, upstream) = client_with_upstream_script(vec![reply], "http").await;
        assert_report_error(
            client
                .send(fulfill_request(
                    "42",
                    r#"{"courier":"BEST","tracking_number":"VP123"}"#,
                    fixture.root_id(),
                    AdminRole::Admin,
                ))
                .await
                .unwrap(),
            status,
            message,
        )
        .await;
        assert_eq!(upstream.requests().len(), 1);
    }
    for flow in ["labels", "fulfill"] {
        for stage in 1..=2 {
            let mut prefix = if flow == "labels" {
                label_prefix(stage)
            } else {
                fulfillment_prefix(stage)
            };
            let operation = match (flow, stage) {
                ("labels", 1) => "tagsAdd",
                ("fulfill", 1) => "fulfillmentCreate",
                _ => "tagsRemove",
            };
            prefix.push(json!({operation:{if operation=="fulfillmentCreate"{"fulfillment"}else{"node"}:null,"userErrors":[{"field":["id"],"message":"private rejected write"}]}}));
            let (client, upstream) = client_with_upstream_script(prefix, "http").await;
            let request = if flow == "labels" {
                authed(
                    "POST",
                    "/v1/admin/orders/shipping_label",
                    fixture.root_id(),
                    AdminRole::Admin,
                )
            } else {
                fulfill_request(
                    "42",
                    r#"{"courier":"BEST","tracking_number":"VP123"}"#,
                    fixture.root_id(),
                    AdminRole::Admin,
                )
            };
            assert_report_error(
                client.send(request).await.unwrap(),
                502,
                "Upstream request failed",
            )
            .await;
            assert_eq!(upstream.requests().len(), stage + 1);
        }
    }
}

#[tokio::test]
async fn shipping_labels_pagination_and_nested_completeness_guard_before_mutations() {
    let fixture = fixture().await;
    for field in ["lineItems", "shippingLines"] {
        let mut order = shipping_order(1, "US", "USPS");
        order[field]["pageInfo"] = json!({"hasNextPage":true,"endCursor":"child"});
        let (client, upstream) =
            client_with_upstream_script(vec![json!({"orders":page(vec![order],None)})], "http")
                .await;
        assert_report_error(
            client
                .send(authed(
                    "POST",
                    "/v1/admin/orders/shipping_label",
                    fixture.root_id(),
                    AdminRole::Admin,
                ))
                .await
                .unwrap(),
            502,
            "Upstream request failed",
        )
        .await;
        assert_eq!(upstream.requests().len(), 1);
    }
    let (client, upstream) = client_with_upstream_script(
        vec![
            json!({"orders":page(vec![shipping_order(1,"US","USPS")],Some("next"))}),
            json!({"orders":page(vec![shipping_order(2,"TH","Standard Shipping")],None)}),
            tags_result(false, 1),
            tags_result(true, 1),
            tags_result(false, 2),
            tags_result(true, 2),
        ],
        "http",
    )
    .await;
    let response = client
        .send(authed(
            "POST",
            "/v1/admin/orders/shipping_label",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_json(response).await;
    assert_eq!(body["fastship"].as_array().unwrap().len(), 1);
    assert_eq!(body["best"].as_array().unwrap().len(), 1);
    assert_eq!(upstream.requests().len(), 6);
}

#[tokio::test]
async fn fulfillment_pages_group_locations_and_stop_before_tags_on_later_failure() {
    let fixture = fixture().await;
    for failure in [false, true] {
        let first = fulfillable_order(Some("next"));
        let mut second = fulfillable_order(None);
        second["order"]["fulfillmentOrders"]["nodes"][0]["id"] =
            json!("gid://shopify/FulfillmentOrder/17");
        second["order"]["fulfillmentOrders"]["nodes"][0]["assignedLocation"]["location"]["id"] =
            json!("gid://shopify/Location/2");
        let created = json!({"fulfillmentCreate":{"fulfillment":{"id":"gid://shopify/Fulfillment/10"},"userErrors":[]}});
        let mut replies = vec![first, second, created.clone()];
        if !failure {
            replies.extend([created, tags_result(true, 42)]);
        }
        let (client, upstream) = client_with_upstream_script(replies, "http").await;
        let response = client
            .send(fulfill_request(
                "42",
                r#"{"courier":"TELEPORT","tracking_number":"T123"}"#,
                fixture.root_id(),
                AdminRole::SuperAdmin,
            ))
            .await
            .unwrap();
        if failure {
            assert_report_error(response, 502, "Upstream request failed").await;
        } else {
            assert_eq!(response.status(), 200);
        }
        let requests = upstream.requests();
        assert_eq!(requests.len(), if failure { 4 } else { 5 });
        assert_eq!(requests[1]["variables"]["after"], "next");
        assert_eq!(requests[2]["operationName"], "CreateFulfillment");
        assert_eq!(requests[3]["operationName"], "CreateFulfillment");
        if !failure {
            assert_eq!(requests[4]["variables"]["tags"], json!(["SHIPPING"]));
        }
    }
}

#[tokio::test]
async fn fulfillment_workflow_rejects_expired_tokens_on_each_route() {
    let client = client().await;
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &json!({"sub":1,"username":"fixture","role":"ADMIN","exp":1}),
        &jsonwebtoken::EncodingKey::from_secret(&hex::decode(crate::common::JWT_SECRET).unwrap()),
    )
    .unwrap();
    for (method, uri) in [
        ("GET", "/v1/admin/orders?tag=X"),
        ("POST", "/v1/admin/orders/shipping_label"),
        ("POST", "/v1/admin/orders/42/fulfill"),
    ] {
        let request = axum::http::Request::builder()
            .method(method)
            .uri(uri)
            .header("authorization", format!("Bearer {token}"))
            .body(axum::body::Body::empty())
            .unwrap();
        assert_report_error(
            client.send(request).await.unwrap(),
            401,
            "Invalid or expired token",
        )
        .await;
    }
}

#[tokio::test]
async fn fulfillment_workflow_invalid_configuration_stays_unavailable() {
    let fixture = fixture().await;
    let client = crate::common::client_with_shopify("invalid-url").await;
    for request in [
        authed(
            "GET",
            "/v1/admin/orders?tag=X",
            fixture.root_id(),
            AdminRole::Admin,
        ),
        authed(
            "POST",
            "/v1/admin/orders/shipping_label",
            fixture.root_id(),
            AdminRole::Admin,
        ),
        fulfill_request(
            "42",
            r#"{"courier":"FASTSHIP","tracking_number":"LR1"}"#,
            fixture.root_id(),
            AdminRole::Admin,
        ),
    ] {
        assert_report_error(
            client.send(request).await.unwrap(),
            503,
            "Shopify report unavailable",
        )
        .await;
    }
}

#[tokio::test]
async fn fulfillment_list_default_and_boundary_pages_return_upstream_metadata() {
    let fixture = fixture().await;
    for (query, size) in [
        ("tag=X", 50),
        ("tag=X&page_size=250", 250),
        ("tag=X&page_size=1", 1),
    ] {
        let (client, upstream) =
            client_with_upstream_script(vec![json!({"orders":page(vec![],None)})], "http").await;
        let response = client
            .send(authed(
                "GET",
                &format!("/v1/admin/orders?{query}"),
                fixture.root_id(),
                AdminRole::SuperAdmin,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            body_json(response).await,
            json!({"orders":[],"page_info":{"has_next_page":false,"end_cursor":null}})
        );
        assert_eq!(upstream.requests()[0]["variables"]["first"], size);
    }
}

#[tokio::test]
async fn fulfillment_nested_truncation_and_unsupported_actions_never_create_fulfillments() {
    let fixture = fixture().await;
    for case in ["nested", "action", "closed", "location"] {
        let mut order = fulfillable_order(None);
        let node = &mut order["order"]["fulfillmentOrders"]["nodes"][0];
        match case {
            "nested" => {
                node["lineItems"]["pageInfo"] = json!({"hasNextPage":true,"endCursor":"child"})
            }
            "action" => node["supportedActions"] = json!([]),
            "closed" => node["status"] = json!("CLOSED"),
            _ => node["assignedLocation"]["location"] = Value::Null,
        };
        let (client, upstream) = client_with_upstream_script(vec![order], "http").await;
        let response = client
            .send(fulfill_request(
                "42",
                r#"{"courier":"BEST","tracking_number":"VP123"}"#,
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        let (status, message) = match case {
            "action" => (409, "Order cannot be fulfilled in its current state"),
            "closed" => (409, "Order has no remaining fulfillable items"),
            _ => (502, "Upstream request failed"),
        };
        assert_report_error(response, status, message).await;
        assert_eq!(upstream.requests().len(), 1);
    }
}

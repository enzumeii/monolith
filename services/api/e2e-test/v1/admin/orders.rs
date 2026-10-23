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
    assert_report_auth(&configured, "/api/v1/admin/orders/report?tags=report").await;
    for role in [AdminRole::Admin, AdminRole::SuperAdmin] {
        for uri in [
            "/api/v1/admin/orders/report",
            "/api/v1/admin/orders/report?tags=",
            "/api/v1/admin/orders/report?tags=%20",
            "/api/v1/admin/orders/report?tags=,",
            "/api/v1/admin/orders/report?tags=SHIPPING,",
            "/api/v1/admin/orders/report?tags=,SHIPPING",
            "/api/v1/admin/orders/report?tags=SHIPPING,%20,TEST",
            "/api/v1/admin/orders/report?tags=report&page_size=1",
            "/api/v1/admin/orders/report?tags=report&cursor=next",
            "/api/v1/admin/orders/report?tags=report&unknown=x",
            "/api/v1/admin/orders/report?tags=report&tags=other",
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
            "/api/v1/admin/orders/report?tags=report",
            fixture.root_id(),
            role,
        )
        .await;
    }
    assert_eq!(upstream.requests(), [] as [serde_json::Value; 0]);
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
            "/api/v1/admin/orders/report?tags=report",
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
            "/api/v1/admin/orders/report?tags=report",
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
        "/api/v1/admin/orders/report?tags=report",
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
                    assert_eq!(v["after"], "orders-2");
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
            "/api/v1/admin/orders/report?tags=%20SHIPPING%20,%20TEST%20",
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
            "/api/v1/admin/orders/report?tags=report",
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
            "/api/v1/admin/orders/report?tags=too-many",
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
            "/api/v1/admin/orders/report?tags=a%3Ab%2Cc%20d",
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
            "/api/v1/admin/orders/report?tags=empty",
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
        json!({"orders":page(vec![json!({"id":"gid://shopify/Order/42","number":1234,"shippingAddress":{"countryCodeV2":"TH"},"email":"buyer@example.test","customer":{"firstName":"Jane","lastName":"Doe"}})],Some("next"))}),
        json!({"orders":page(vec![],None)}),
    ], "http").await;
    let response = client.send(authed("GET", "/api/v1/admin/orders?tag=SHIPPING,FASTSHIP&page_size=1&cursor=prev&fulfillment=UNFULFILLED", fixture.root_id(), AdminRole::Admin)).await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        body_json(response).await,
        json!({"orders":[{"id":42,"number":1234,"full_name":"Jane Doe","email":"buyer@example.test","shipping_country_code":"TH"}],"page_info":{"has_next_page":true,"end_cursor":"next"}})
    );
    let response = client
        .send(authed(
            "POST",
            "/api/v1/admin/orders/shipping_label",
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
        json!({"first":1,"after":"prev","search":"tag:\"SHIPPING\" AND tag:\"FASTSHIP\" AND fulfillment_status:unfulfilled","includeContact":true,"includeTags":false})
    );
    assert_eq!(
        requests[1]["variables"]["search"],
        "status:open AND financial_status:paid AND fulfillment_status:unfulfilled AND tag:\"PACKING\""
    );
}

fn shipping_order(id: u64, country: &str, method: &str) -> Value {
    json!({"id":format!("gid://shopify/Order/{id}"),"email":"customer@example.test","totalWeight":2000,
        "shippingAddress":{"firstName":"Jane","lastName":"Doe","name":"Jane Doe","address1":"1 Test St","address2":null,"city":"Test City","provinceCode":"TS","zip":"00123","country":"Test Country","countryCodeV2":country,"phone":"+66 (123) 456"},
        "shippingLines":{"nodes":vec![json!({"code":method})]},
        "lineItems":{"nodes":vec![json!({"title":"Keychain","sku":"ABCKEY000001","quantity":3,"originalUnitPriceSet":{"shopMoney":{"amount":"30","currencyCode":"THB"}}})]}})
}
fn tag_read(id: u64, remove: bool) -> Value {
    let tags = if remove {
        vec![if id == 42 { "SHIPPING" } else { "PACKING" }]
    } else {
        vec![]
    };
    json!({"orders":page(vec![json!({"id":format!("gid://shopify/Order/{id}"),"name":format!("#{id}"),"tags":tags})],None)})
}
fn tags_result(remove: bool, id: u64) -> Value {
    json!({if remove {"tagsRemove"} else {"tagsAdd"}:{"node":{"id":format!("gid://shopify/Order/{id}")},"userErrors":[]}})
}
fn fulfillable_order(next: Option<&str>) -> Value {
    json!({"order":{"id":"gid://shopify/Order/42","fulfillmentOrders":page(vec![json!({"id":"gid://shopify/FulfillmentOrder/7","status":"OPEN","supportedActions":[{"action":"CREATE_FULFILLMENT"}],"lineItems":{"nodes":vec![json!({"id":"gid://shopify/FulfillmentOrderLineItem/8","remainingQuantity":2}),json!({"id":"gid://shopify/FulfillmentOrderLineItem/9","remainingQuantity":0})]}})],next)}})
}
#[tokio::test]
async fn shipping_labels_route_preserves_reference_carriers_and_tag_order() {
    let fixture = fixture().await;
    let mut orders = vec![
        shipping_order(1, "US", "Standard Shipping"),
        shipping_order(2, "AU", "Tracking number"),
        shipping_order(3, "TH", "Standard Shipping"),
        shipping_order(4, "SG", "TELEPORT AIRASIA : TRACKING & FAST"),
    ];
    orders[0]["email"] = Value::Null;
    let mut replies = vec![json!({"orders":page(orders,None)})];
    for id in 1..=4 {
        replies.extend([
            tag_read(id, false),
            tags_result(false, id),
            tag_read(id, true),
            tags_result(true, id),
        ]);
    }
    let (client, upstream) = client_with_upstream_script(replies, "http").await;
    let response = client
        .send(authed(
            "POST",
            "/api/v1/admin/orders/shipping_label",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_json(response).await;
    assert_eq!(body["fastship"][0]["email"], "shipping@enzumeii.com");
    assert_eq!(body["fastship"][0]["quantity"], 3);
    assert_eq!(body["fastship"][0]["declared_value"], 150.0);
    assert_eq!(body["fastship"][0]["incoterm"], "DDU");
    assert_eq!(body["fastship"][0]["phone"], "66123456");
    assert_eq!(body["thaipost"][0]["service_id"], 3278);
    assert_eq!(body["thaipost"][0]["sender_postcode"], "10100");
    assert_eq!(body["best"][0]["weight"], 0);
    assert_eq!(body["teleport"][0]["weight"], 2.0);
    assert_eq!(
        body["teleport"][0]["is_multi_piece"],
        "Multi Piece Shippment"
    );
    let requests = upstream.requests();
    assert_eq!(requests.len(), 17);
    for (i, courier) in ["FASTSHIP", "THAI_POST", "BEST", "TELEPORT"]
        .iter()
        .enumerate()
    {
        assert_eq!(requests[2 + i * 4]["operationName"], "EditOrderTags");
        assert_eq!(requests[2 + i * 4]["variables"]["add"], true);
        assert_eq!(
            requests[2 + i * 4]["variables"]["tags"],
            json!(["SHIPPING", courier])
        );
        assert_eq!(requests[4 + i * 4]["operationName"], "EditOrderTags");
        assert_eq!(requests[4 + i * 4]["variables"]["add"], false);
        assert_eq!(requests[4 + i * 4]["variables"]["tags"], json!(["PACKING"]));
    }
}
#[tokio::test]
async fn fulfill_route_fulfills_remaining_lines_before_removing_shipping() {
    let fixture = fixture().await;
    let (client,upstream)=client_with_upstream_script(vec![fulfillable_order(None),json!({"fulfillmentCreate":{"fulfillment":{"id":"gid://shopify/Fulfillment/10"},"userErrors":[]}}),tag_read(42, true), tags_result(true,42)],"http").await;
    let mut request = authed(
        "POST",
        "/api/v1/admin/orders/42/fulfill",
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
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests[1]["variables"]["fulfillment"],
        json!({"notifyCustomer":true,"trackingInfo":{"company":"Other","number":"LR123","url":"https://track.fastship.co/LR123"},"lineItemsByFulfillmentOrder":[{"fulfillmentOrderId":"gid://shopify/FulfillmentOrder/7","fulfillmentOrderLineItems":[{"id":"gid://shopify/FulfillmentOrderLineItem/8","quantity":2}]}]})
    );
    assert_eq!(requests[3]["variables"]["tags"], json!(["SHIPPING"]));
}

fn fulfill_request(
    id: &str,
    body: &str,
    admin: i64,
    role: AdminRole,
) -> axum::http::Request<axum::body::Body> {
    let mut request = authed(
        "POST",
        &format!("/api/v1/admin/orders/{id}/fulfill"),
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
    let message = "Expected tag, a valid fulfillment, page_size between 1 and 250, and an optional nonblank cursor";
    for query in [
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
                &format!("/api/v1/admin/orders{query}"),
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
    assert_eq!(upstream.requests(), [] as [serde_json::Value; 0]);
}

#[tokio::test]
async fn fulfillment_workflow_auth_and_optional_dependency_over_tcp() {
    let fixture = fixture().await;
    let client = client().await;
    for (method, uri) in [
        ("GET", "/api/v1/admin/orders?tag=X"),
        ("POST", "/api/v1/admin/orders/shipping_label"),
        ("POST", "/api/v1/admin/orders/42/fulfill"),
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
                "Shopify client unavailable",
            )
            .await;
        }
    }
}

fn fulfillment_prefix(stage: usize) -> Vec<Value> {
    vec![fulfillable_order(None), json!({"fulfillmentCreate":{"fulfillment":{"id":"gid://shopify/Fulfillment/10"},"userErrors":[]}}), tag_read(42, true)].into_iter().take(stage).collect()
}
fn label_prefix(stage: usize) -> Vec<Value> {
    vec![
        json!({"orders":page(vec![shipping_order(1,"US","Standard Shipping")],None)}),
        tag_read(1, false),
        tags_result(false, 1),
        tag_read(1, true),
    ]
    .into_iter()
    .take(stage)
    .collect()
}

#[tokio::test]
async fn fulfillment_workflow_each_upstream_operation_maps_failures_and_stops() {
    let fixture = fixture().await;
    for flow in ["list", "labels", "fulfill"] {
        let stages = match flow {
            "list" => 1,
            "labels" => 5,
            _ => 4,
        };
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
                        "/api/v1/admin/orders/shipping_label",
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
                        "/api/v1/admin/orders?tag=X",
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
            "/api/v1/admin/orders?tag=X",
            fixture.root_id(),
            AdminRole::Admin,
        ),
        authed(
            "POST",
            "/api/v1/admin/orders/shipping_label",
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
                    "/api/v1/admin/orders/shipping_label",
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
                "/api/v1/admin/orders/shipping_label",
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
        for stage in if flow == "labels" { [2, 4] } else { [1, 3] } {
            let mut prefix = if flow == "labels" {
                label_prefix(stage)
            } else {
                fulfillment_prefix(stage)
            };
            let operation = match (flow, stage) {
                ("labels", 2) => "tagsAdd",
                ("fulfill", 1) => "fulfillmentCreate",
                _ => "tagsRemove",
            };
            prefix.push(json!({operation:{if operation=="fulfillmentCreate"{"fulfillment"}else{"node"}:null,"userErrors":[{"field":["id"],"message":"private rejected write"}]}}));
            let (client, upstream) = client_with_upstream_script(prefix, "http").await;
            let request = if flow == "labels" {
                authed(
                    "POST",
                    "/api/v1/admin/orders/shipping_label",
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
async fn shipping_labels_root_pagination_precedes_mutations() {
    let fixture = fixture().await;
    let (client, upstream) = client_with_upstream_script(
        vec![
            json!({"orders":page(vec![shipping_order(1,"US","USPS")],Some("next"))}),
            json!({"orders":page(vec![shipping_order(2,"TH","Standard Shipping")],None)}),
            tag_read(1, false),
            tags_result(false, 1),
            tag_read(1, true),
            tags_result(true, 1),
            tag_read(2, false),
            tags_result(false, 2),
            tag_read(2, true),
            tags_result(true, 2),
        ],
        "http",
    )
    .await;
    let response = client
        .send(authed(
            "POST",
            "/api/v1/admin/orders/shipping_label",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_json(response).await;
    assert_eq!(body["fastship"].as_array().unwrap().len(), 1);
    assert_eq!(body["best"].as_array().unwrap().len(), 1);
    assert_eq!(upstream.requests().len(), 10);
}

#[tokio::test]
async fn fulfillment_short_pages_combine_orders_and_stop_before_tags_on_creation_failure() {
    for failure in [false, true] {
        let first = fulfillable_order(Some("next"));
        let mut second = fulfillable_order(None);
        second["order"]["fulfillmentOrders"]["nodes"][0]["id"] =
            json!("gid://shopify/FulfillmentOrder/17");
        second["order"]["fulfillmentOrders"]["nodes"][0]["lineItems"]["nodes"][0]["id"] =
            json!("gid://shopify/FulfillmentOrderLineItem/18");
        let mut replies = vec![first, second];
        if !failure {
            replies.extend([json!({"fulfillmentCreate":{"fulfillment":{"id":"gid://shopify/Fulfillment/10"},"userErrors":[]}}),tag_read(42, true), tags_result(true,42)]);
        }
        let (client, upstream) = client_with_upstream_script(replies, "http").await;
        let response = client
            .send(fulfill_request(
                "42",
                r#"{"courier":"BEST","tracking_number":"VP123"}"#,
                1,
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        if failure {
            assert_report_error(response, 502, "Upstream request failed").await;
        } else {
            assert_eq!(response.status(), 200);
        }
        let requests = upstream.requests();
        assert_eq!(requests.len(), if failure { 3 } else { 5 });
        assert_eq!(requests[1]["variables"]["after"], "next");
        assert_eq!(
            requests[2]["variables"]["fulfillment"]["lineItemsByFulfillmentOrder"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
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
        ("GET", "/api/v1/admin/orders?tag=X"),
        ("POST", "/api/v1/admin/orders/shipping_label"),
        ("POST", "/api/v1/admin/orders/42/fulfill"),
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
            "/api/v1/admin/orders?tag=X",
            fixture.root_id(),
            AdminRole::Admin,
        ),
        authed(
            "POST",
            "/api/v1/admin/orders/shipping_label",
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
            "Shopify client unavailable",
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
                &format!("/api/v1/admin/orders?{query}"),
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
async fn fulfillment_unsupported_actions_and_closed_orders_never_create_fulfillments() {
    let fixture = fixture().await;
    for case in ["action", "closed"] {
        let mut order = fulfillable_order(None);
        let node = &mut order["order"]["fulfillmentOrders"]["nodes"][0];
        match case {
            "action" => node["supportedActions"] = json!([]),
            "closed" => node["status"] = json!("CLOSED"),
            _ => unreachable!(),
        }
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

#[tokio::test]
async fn review_regression_rejects_bad_carrier_contact_and_dot_tracking_before_writes() {
    let fixture = fixture().await;
    for (country, method, field, value, message) in [
        (
            "TH",
            "Standard Shipping",
            "phone",
            Value::Null,
            "Order shipping phone missing",
        ),
        (
            "SG",
            "TELEPORT AIRASIA : TRACKING & FAST",
            "phone",
            json!("++ ( )"),
            "Order shipping phone missing",
        ),
        (
            "TH",
            "Standard Shipping",
            "zip",
            json!(" "),
            "Order shipping postcode missing",
        ),
        (
            "SG",
            "TELEPORT AIRASIA : TRACKING & FAST",
            "zip",
            Value::Null,
            "Order shipping postcode missing",
        ),
        (
            "SG",
            "DHL Express",
            "phone",
            json!("123"),
            "Order has no supported shipping method",
        ),
    ] {
        let mut order = shipping_order(1, country, method);
        order["shippingAddress"][field] = value;
        let (client, upstream) =
            client_with_upstream_script(vec![json!({"orders":page(vec![order],None)})], "http")
                .await;
        let response = client
            .send(authed(
                "POST",
                "/api/v1/admin/orders/shipping_label",
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_report_error(
            response,
            if method == "DHL Express" { 409 } else { 502 },
            message,
        )
        .await;
        assert_eq!(upstream.requests().len(), 1);
    }
    let (client, upstream) = client_with_upstream_script(vec![], "http").await;
    for tracking in [".", ".."] {
        let body = json!({"courier":"FASTSHIP","tracking_number":tracking}).to_string();
        let response = client
            .send(fulfill_request(
                "42",
                &body,
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    assert_eq!(upstream.requests(), [] as [serde_json::Value; 0]);
}

#[tokio::test]
async fn review_regression_unknown_shipping_code_never_changes_tags() {
    let fixture = fixture().await;
    let (client, upstream) = client_with_upstream_script(
        vec![json!({"orders":page(vec![shipping_order(1,"SG","DHL Express")],None)})],
        "http",
    )
    .await;
    let response = client
        .send(authed(
            "POST",
            "/api/v1/admin/orders/shipping_label",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_report_error(response, 409, "Order has no supported shipping method").await;
    assert_eq!(upstream.requests().len(), 1);
}
#[tokio::test]
async fn review_regression_dot_tracking_never_reaches_shopify() {
    let fixture = fixture().await;
    let (client, upstream) = client_with_upstream_script(vec![], "http").await;
    for number in [".", ".."] {
        let response = client
            .send(fulfill_request(
                "42",
                &json!({"courier":"BEST","tracking_number":number}).to_string(),
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 400);
    }
    assert_eq!(upstream.requests(), [] as [serde_json::Value; 0]);
}

#[tokio::test]
async fn combined_queries_read_250_items_and_shipping_lines_without_continuation() {
    let fixture = fixture().await;
    let mut order = shipping_order(1, "US", "USPS");
    let item = order["lineItems"]["nodes"][0].clone();
    order["lineItems"] = json!({"nodes":vec![item;250]});
    order["shippingLines"] = json!({"nodes":vec![json!({"code":"USPS"});250]});
    let (client, upstream) = client_with_upstream_script(
        vec![
            json!({"orders":page(vec![order],None)}),
            tag_read(1, false),
            tags_result(false, 1),
            tag_read(1, true),
            tags_result(true, 1),
        ],
        "http",
    )
    .await;
    let response = client
        .send(authed(
            "POST",
            "/api/v1/admin/orders/shipping_label?x=1",
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(body_json(response).await["fastship"][0]["quantity"], 750);
    let calls = upstream.requests();
    assert_eq!(calls.len(), 5);
    assert_eq!(calls[0]["operationName"], "ShippingOrders");
    assert_eq!(calls[2]["operationName"], "EditOrderTags");
    assert!(
        calls[0]["query"]
            .as_str()
            .unwrap()
            .contains("shippingLines(first: 250)")
    );
    assert!(
        calls[0]["query"]
            .as_str()
            .unwrap()
            .contains("lineItems(first: 250)")
    );
    let mut order = fulfillable_order(None);
    order["order"]["fulfillmentOrders"]["nodes"][0]["lineItems"] = json!({"nodes":(1..=250).map(|id|json!({"id":format!("gid://shopify/FulfillmentOrderLineItem/{id}"),"remainingQuantity":1})).collect::<Vec<_>>()});
    let (client,upstream)=client_with_upstream_script(vec![order,json!({"fulfillmentCreate":{"fulfillment":{"id":"gid://shopify/Fulfillment/10"},"userErrors":[]}}),tag_read(42, true), tags_result(true,42)],"http").await;
    let response = client
        .send(fulfill_request(
            "42",
            r#"{"courier":"BEST","tracking_number":"VP123"}"#,
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let requests = upstream.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[1]["variables"]["fulfillment"]["lineItemsByFulfillmentOrder"][0]["fulfillmentOrderLineItems"].as_array().unwrap().len(),250);
    assert_eq!(requests[3]["variables"]["add"], false);
}

struct ReshipMailServer {
    url: String,
    requests: std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for ReshipMailServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl ReshipMailServer {
    async fn start(mode: &'static str) -> Self {
        Self::start_with_check(mode, || {}).await
    }
    async fn start_with_check(
        mode: &'static str,
        before_reply: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        use axum::http::{HeaderMap, StatusCode};
        let before_reply = std::sync::Arc::new(before_reply);
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = requests.clone();
        let router = Router::new().route("/emails", axum::routing::post(move |headers: HeaderMap, Json(body): Json<Value>| {
            let recorded = recorded.clone();
            let before_reply = before_reply.clone();
            async move {
                before_reply();
                assert_eq!(headers["authorization"], "Bearer re_e2e-fixture");
                assert!(!headers.contains_key("idempotency-key"));
                recorded.lock().unwrap().push(body);
                match mode {
                    "ok" => Json(json!({"id":"email-fixture-id"})).into_response(),
                    "blank" => Json(json!({"id":"  "})).into_response(),
                    "missing" => Json(json!({})).into_response(),
                    "malformed" => "{".into_response(),
                    "timeout" => { tokio::time::sleep(std::time::Duration::from_secs(3)).await; Json(json!({"id":"late"})).into_response() },
                    status => (StatusCode::from_u16(status.parse().unwrap()).unwrap(), Json(json!({"statusCode":status.parse::<u16>().unwrap(),"name":"validation_error","message":"private recipient@example.test re_secret body"}))).into_response(),
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            url,
            requests,
            task,
        }
    }
    fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
}
fn reship_order() -> Value {
    json!({"orders":page(vec![json!({"id":"gid://shopify/Order/42","number":1085,"email":"recipient@example.test"})],None)})
}
fn reship_request() -> axum::http::Request<axum::body::Body> {
    authed(
        "POST",
        "/api/v1/admin/orders/1085/reship",
        1,
        AdminRole::Admin,
    )
}
async fn reship_client(shopify_url: &str, mail: &ReshipMailServer) -> crate::common::TestClient {
    crate::common::client_with_providers(crate::common::ProviderConfig {
        shopify_url: Some(shopify_url),
        paypal_url: None,
        stripe_url: None,
        resend_url: Some(&mail.url),
        resend_key: Some("re_e2e-fixture"),
        email_timeout_secs: Some(1),
        ..Default::default()
    })
    .await
}

#[tokio::test]
async fn reship_sends_reference_notice_before_tagging_and_each_explicit_post() {
    let (url, upstream) = crate::common::scripted_upstream(
        vec![
            reship_order(),
            tag_read(42, false),
            tags_result(false, 42),
            reship_order(),
            tag_read(42, false),
            tags_result(false, 42),
        ],
        "http",
    )
    .await;
    let upstream = std::sync::Arc::new(upstream);
    let lookup = upstream.clone();
    let mail = ReshipMailServer::start_with_check("ok", move || {
        let calls = lookup.requests();
        assert_eq!(calls.last().unwrap()["operationName"], "ListOrders");
        assert_eq!(calls.last().unwrap()["variables"]["includeContact"], true);
        assert_eq!(calls.last().unwrap()["variables"]["includeTags"], false);
    })
    .await;
    let client = reship_client(&url, &mail).await;
    for (index, role) in [AdminRole::Admin, AdminRole::SuperAdmin]
        .into_iter()
        .enumerate()
    {
        let response = client
            .send(authed("POST", "/api/v1/admin/orders/1085/reship", 1, role))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            body_json(response).await,
            json!({"success":true,"email_id":"email-fixture-id"})
        );
        let calls = upstream.requests();
        assert_eq!(calls.len(), (index + 1) * 3);
        assert_eq!(calls[index * 3]["operationName"], "ListOrders");
        assert_eq!(
            calls[index * 3]["variables"],
            json!({"first":1,"after":null,"search":"name:1085","includeContact":true,"includeTags":false})
        );
        assert_eq!(calls[index * 3 + 2]["operationName"], "EditOrderTags");
        assert_eq!(
            calls[index * 3 + 2]["variables"],
            json!({"id":"gid://shopify/Order/42","tags":["RETURNED"],"add":true})
        );
        let sends = mail.requests();
        assert_eq!(sends.len(), index + 1);
        assert_eq!(sends[index]["from"], "shipping@enzumeii.com");
        assert_eq!(sends[index]["to"], json!(["recipient@example.test"]));
        assert_eq!(
            sends[index]["subject"],
            "[ENZUMEII] Urgent : Reshipping your returned package order #1085"
        );
        let html = sends[index]["html"].as_str().unwrap();
        for phrase in [
            "order #1085",
            "Hello from Automated Zuzu",
            "85% of the item price",
            "The shipping fee will not be included",
            "verifying and updating your delivery information",
            "invoice you just the shipping fee again",
            "reply directly to this email",
            "https://twitter.com/enzumeii",
            "https://www.instagram.com/enzumeii",
        ] {
            assert!(html.contains(phrase), "missing {phrase}");
        }
        assert!(!html.contains("{order_no}"));
        assert!(sends[index].get("reply_to").is_none());
    }
}

#[tokio::test]
async fn reship_rejects_each_invalid_input_without_provider_calls() {
    let (url, upstream) = crate::common::scripted_upstream(vec![], "http").await;
    let mail = ReshipMailServer::start("ok").await;
    let client = reship_client(&url, &mail).await;
    for id in [
        "abc",
        "0",
        "-1",
        "999",
        "10000",
        "0001",
        "+1085",
        "%201085",
        "１０８５",
        "18446744073709551616",
        "%FF",
    ] {
        let response = client
            .send(authed(
                "POST",
                &format!("/api/v1/admin/orders/{id}/reship"),
                1,
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_report_error(
            response,
            400,
            "Expected a four-digit order number (1000-9999) without query parameters or a request body",
        )
        .await;
    }
    for (query, body) in [("?x=1", ""), ("", "{}"), ("", "{"), ("", " ")] {
        let mut request = authed(
            "POST",
            &format!("/api/v1/admin/orders/1085/reship{query}"),
            1,
            AdminRole::Admin,
        );
        *request.body_mut() = axum::body::Body::from(body);
        assert_report_error(
            client.send(request).await.unwrap(),
            400,
            "Expected a four-digit order number (1000-9999) without query parameters or a request body",
        )
        .await;
    }
    assert_eq!(upstream.requests(), [] as [serde_json::Value; 0]);
    assert_eq!(mail.requests(), [] as [serde_json::Value; 0]);
}

#[tokio::test]
async fn reship_auth_and_cookie_origin_rejections_precede_providers() {
    let (url, upstream) = crate::common::scripted_upstream(vec![], "http").await;
    let mail = ReshipMailServer::start("ok").await;
    let client = reship_client(&url, &mail).await;
    let expired = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &json!({"sub":1,"username":"fixture","role":"ADMIN","exp":1}),
        &jsonwebtoken::EncodingKey::from_secret(&hex::decode(crate::common::JWT_SECRET).unwrap()),
    )
    .unwrap();
    for token in [None, Some("invalid".to_owned()), Some(expired)] {
        let mut request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/admin/orders/1085/reship");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        let response = client
            .send(request.body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
        assert_report_error(response, 401, "Invalid or expired token").await;
    }
    let cookie = format!(
        "backoffice_auth={}~{}",
        crate::common::jwt_for(1, AdminRole::Admin),
        "ab".repeat(32)
    );
    for origin in [None, Some("https://untrusted.example.test")] {
        let mut request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/admin/orders/1085/reship")
            .header("cookie", &cookie);
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let response = client
            .send(request.body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), 403);
    }
    assert_eq!(upstream.requests(), [] as [serde_json::Value; 0]);
    assert_eq!(mail.requests(), [] as [serde_json::Value; 0]);
}

#[tokio::test]
async fn reship_domain_and_unusable_contacts_never_tag_or_send() {
    let mut cases = vec![
        (json!({"orders":page(vec![],None)}), 404, "Order not found"),
        (json!({}), 502, "Upstream request failed"),
        (json!({"orders":false}), 502, "Upstream request failed"),
    ];
    for email in [Value::Null, json!(""), json!("  ")] {
        let mut data = reship_order();
        data["orders"]["nodes"][0]["email"] = email;
        cases.push((data, 409, "Order email missing"));
    }
    {
        let (field, value) = ("number", json!("1085"));
        let mut data = reship_order();
        data["orders"]["nodes"][0][field] = value;
        cases.push((data, 502, "Upstream request failed"));
    }
    for field in ["id", "number", "email"] {
        let mut data = reship_order();
        data["orders"]["nodes"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        cases.push((data, 502, "Upstream request failed"));
    }
    let mail = ReshipMailServer::start("ok").await;
    for (data, status, message) in cases {
        let (url, upstream) = crate::common::scripted_upstream(vec![data], "http").await;
        let client = reship_client(&url, &mail).await;
        assert_report_error(
            client.send(reship_request()).await.unwrap(),
            status,
            message,
        )
        .await;
        assert_eq!(upstream.requests().len(), 1);
    }
    assert_eq!(mail.requests(), [] as [serde_json::Value; 0]);
}

#[tokio::test]
async fn reship_optional_clients_and_settings_fail_before_lookup() {
    let (url, upstream) = crate::common::scripted_upstream(vec![], "http").await;
    let mail = ReshipMailServer::start("ok").await;
    for (shopify, key, timeout, message) in [
        (
            None,
            Some("re_e2e-fixture"),
            Some(1),
            "Shopify client unavailable",
        ),
        (
            Some("invalid-url"),
            Some("re_e2e-fixture"),
            Some(1),
            "Shopify client unavailable",
        ),
        (
            Some(url.as_str()),
            None,
            Some(1),
            "Resend client unavailable",
        ),
        (
            Some(url.as_str()),
            Some(""),
            Some(1),
            "Resend client unavailable",
        ),
        (
            Some(url.as_str()),
            Some("re_e2e-fixture"),
            None,
            "Resend client unavailable",
        ),
        (
            Some(url.as_str()),
            Some("re_e2e-fixture"),
            Some(0),
            "Resend client unavailable",
        ),
    ] {
        let client = crate::common::client_with_providers(crate::common::ProviderConfig {
            shopify_url: shopify,
            paypal_url: None,
            stripe_url: None,
            resend_url: Some(&mail.url),
            resend_key: key,
            email_timeout_secs: timeout,
            ..Default::default()
        })
        .await;
        assert_report_error(client.send(reship_request()).await.unwrap(), 503, message).await;
        assert_eq!(
            client
                .send(authed("GET", "/healthz", 1, AdminRole::Admin))
                .await
                .unwrap()
                .status(),
            200
        );
    }
    assert_eq!(upstream.requests(), [] as [serde_json::Value; 0]);
    assert_eq!(mail.requests(), [] as [serde_json::Value; 0]);
}

#[tokio::test]
async fn reship_lookup_and_tag_failures_preserve_email_ordering() {
    let mail = ReshipMailServer::start("ok").await;
    for stage in 0..3 {
        for failure in [
            "http",
            "graphql",
            "partial",
            "malformed",
            "missing",
            "shape",
            "throttled",
            "timeout",
        ] {
            let prefix = vec![reship_order(), tag_read(42, false)]
                .into_iter()
                .take(stage)
                .collect();
            let sent_before = mail.requests().len();
            let (url, upstream) = crate::common::scripted_upstream(prefix, failure).await;
            let client = reship_client(&url, &mail).await;
            assert_report_error(
                client.send(reship_request()).await.unwrap(),
                if failure == "timeout" { 504 } else { 502 },
                "Upstream request failed",
            )
            .await;
            assert_eq!(
                upstream.requests().len(),
                stage + if failure == "throttled" { 4 } else { 1 },
                "{failure} stage={stage}"
            );
            assert_eq!(mail.requests().len() - sent_before, usize::from(stage > 0));
        }
    }
}

#[tokio::test]
async fn reship_unusable_tag_results_fail_after_email_acceptance() {
    let mail = ReshipMailServer::start("ok").await;
    for data in [
        json!({"tagsAdd":{"node":null,"userErrors":[]}}),
        json!({"tagsAdd":{"node":{"id":"gid://shopify/Order/42"},"userErrors":[{"field":["tags"],"message":"private recipient@example.test"}]}}),
        json!({"tagsAdd":{}}),
    ] {
        let (url, upstream) = crate::common::scripted_upstream(
            vec![reship_order(), tag_read(42, false), data],
            "http",
        )
        .await;
        let sent_before = mail.requests().len();
        let client = reship_client(&url, &mail).await;
        assert_report_error(
            client.send(reship_request()).await.unwrap(),
            502,
            "Upstream request failed",
        )
        .await;
        assert_eq!(upstream.requests().len(), 3);
        assert_eq!(mail.requests().len() - sent_before, 1);
    }
}

#[tokio::test]
async fn reship_email_failures_never_tag_or_retry() {
    for failure in [
        "401",
        "422",
        "429",
        "500",
        "503",
        "malformed",
        "missing",
        "blank",
        "timeout",
    ] {
        let (url, upstream) = crate::common::scripted_upstream(vec![reship_order()], "http").await;
        let mail = ReshipMailServer::start(failure).await;
        let client = reship_client(&url, &mail).await;
        let started = std::time::Instant::now();
        assert_report_error(
            client.send(reship_request()).await.unwrap(),
            if failure == "timeout" { 504 } else { 502 },
            "Upstream request failed",
        )
        .await;
        if failure == "timeout" {
            assert!(started.elapsed() < std::time::Duration::from_secs(3));
        }
        assert_eq!(mail.requests().len(), 1, "{failure}");
        let calls = upstream.requests();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0]["operationName"], "ListOrders");
        let logs = client.logs();
        assert!(logs.contains("API operation failed"));
        for private in [
            "recipient@example.test",
            "re_secret",
            "re_e2e-fixture",
            "private recipient",
            "Automated Zuzu",
        ] {
            assert!(!logs.contains(private), "ordinary log leaked {private}");
        }
    }
}

struct OrderFaultServer {
    url: String,
    received: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for OrderFaultServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl OrderFaultServer {
    async fn start(later: bool, disconnect: bool) -> Self {
        Self::script(
            if later { vec![reship_order()] } else { vec![] },
            disconnect,
        )
        .await
    }
    async fn script(prefix: Vec<Value>, disconnect: bool) -> Self {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let received = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = received.clone();
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let mut prefix = std::collections::VecDeque::from(prefix);
            if prefix.is_empty() && !disconnect {
                drop(listener);
                ready_tx.send(()).unwrap();
                return;
            }
            ready_tx.send(()).unwrap();
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let mut buf = [0; 4096];
                tokio::time::timeout(std::time::Duration::from_secs(5), async {
                    loop {
                        let n = stream.read(&mut buf).await.unwrap();
                        if n == 0 {
                            break;
                        }
                        bytes.extend_from_slice(&buf[..n]);
                        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                            let headers = String::from_utf8_lossy(&bytes[..end]);
                            let length = headers
                                .lines()
                                .find_map(|line| {
                                    line.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|value| value.trim().parse::<usize>().unwrap())
                                })
                                .unwrap_or(0);
                            if bytes.len() >= end + 4 + length {
                                break;
                            }
                        }
                    }
                })
                .await
                .unwrap();
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if let Some(data) = prefix.pop_front() {
                    let body = json!({"data":data}).to_string();
                    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
                    stream.shutdown().await.unwrap();
                    if prefix.is_empty() && !disconnect {
                        break;
                    }
                }
                // Otherwise drop the socket after receiving the real request.
            }
        });
        ready_rx.await.unwrap();
        Self {
            url,
            received,
            task,
        }
    }
    fn count(&self) -> usize {
        self.received.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[tokio::test]
async fn reship_connection_refusal_and_disconnect_map_at_each_dependency_stage() {
    for disconnect in [false, true] {
        for stage in 0..3 {
            let prefix = vec![reship_order(), tag_read(42, false)]
                .into_iter()
                .take(stage)
                .collect();
            let fault = OrderFaultServer::script(prefix, disconnect).await;
            let mail = ReshipMailServer::start("ok").await;
            let client = reship_client(&format!("{}/graphql.json", fault.url), &mail).await;
            assert_report_error(
                client.send(reship_request()).await.unwrap(),
                502,
                "Upstream request failed",
            )
            .await;
            assert_eq!(fault.count(), stage + usize::from(disconnect));
            assert_eq!(mail.requests().len(), usize::from(stage > 0));
        }
        let (url, upstream) = crate::common::scripted_upstream(vec![reship_order()], "http").await;
        let fault = OrderFaultServer::start(false, disconnect).await;
        let client = crate::common::client_with_providers(crate::common::ProviderConfig {
            shopify_url: Some(&url),
            paypal_url: None,
            stripe_url: None,
            resend_url: Some(&fault.url),
            resend_key: Some("re_e2e-fixture"),
            email_timeout_secs: Some(1),
            ..Default::default()
        })
        .await;
        assert_report_error(
            client.send(reship_request()).await.unwrap(),
            502,
            "Upstream request failed",
        )
        .await;
        assert_eq!(fault.count(), usize::from(disconnect));
        assert_eq!(upstream.requests().len(), 1);
    }
}

#[tokio::test]
async fn review_connection_faults_stop_at_every_workflow_operation() {
    let mut cases = vec![("list", vec![])];
    for stage in 0..5 {
        cases.push(("labels", label_prefix(stage)));
    }
    for stage in 0..4 {
        cases.push(("fulfill", fulfillment_prefix(stage)));
    }
    for disconnect in [false, true] {
        for (flow, prefix) in &cases {
            let completed = prefix.len();
            let server = OrderFaultServer::script(prefix.clone(), disconnect).await;
            let client =
                crate::common::client_with_shopify(&format!("{}/graphql.json", server.url)).await;
            let request = match *flow {
                "labels" => authed(
                    "POST",
                    "/api/v1/admin/orders/shipping_label",
                    1,
                    AdminRole::Admin,
                ),
                "fulfill" => fulfill_request(
                    "42",
                    r#"{"courier":"THAI_POST","tracking_number":"RR123TH"}"#,
                    1,
                    AdminRole::Admin,
                ),
                _ => authed("GET", "/api/v1/admin/orders?tag=X", 1, AdminRole::Admin),
            };
            assert_report_error(
                client.send(request).await.unwrap(),
                502,
                "Upstream request failed",
            )
            .await;
            assert_eq!(
                server.count(),
                completed + usize::from(disconnect),
                "{flow} after{completed}, disconnect={disconnect}"
            );
        }
    }
}

#[tokio::test]
async fn fulfillment_location_batches_combine_same_location_across_pages() {
    let fixture = fixture().await;
    for split_pages in [false, true] {
        let mut first = fulfillable_order(split_pages.then_some("next"));
        let mut second = fulfillable_order(None);
        let second_node = &mut second["order"]["fulfillmentOrders"]["nodes"][0];
        second_node["id"] = json!("gid://shopify/FulfillmentOrder/17");
        second_node["lineItems"]["nodes"][0]["id"] =
            json!("gid://shopify/FulfillmentOrderLineItem/18");
        let mut replies = if split_pages {
            vec![first, second]
        } else {
            first["order"]["fulfillmentOrders"]["nodes"]
                .as_array_mut()
                .unwrap()
                .push(second_node.clone());
            vec![first]
        };
        let reads = replies.len();
        replies.extend([
            json!({"fulfillmentCreate":{"fulfillment":{"id":"gid://shopify/Fulfillment/10"},"userErrors":[]}}),
            tag_read(42, true), tags_result(true, 42),
        ]);
        let (client, upstream) = client_with_upstream_script(replies, "http").await;
        let response = client
            .send(fulfill_request(
                "42",
                r#"{"courier":"BEST","tracking_number":"VP123"}"#,
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200, "split_pages={split_pages}");
        assert_eq!(
            body_json(response).await,
            json!({"success":true,"tracking_url":"https://www.aftership.com/track/800bestex/VP123"})
        );
        let requests = upstream.requests();
        assert_eq!(requests.len(), reads + 3);
        let input = &requests[reads]["variables"]["fulfillment"];
        assert_eq!(requests[reads]["operationName"], "CreateFulfillment");
        assert_eq!(input["notifyCustomer"], true);
        assert_eq!(
            input["trackingInfo"],
            json!({"company":"Other","number":"VP123","url":"https://www.aftership.com/track/800bestex/VP123"})
        );
        assert_eq!(
            input["lineItemsByFulfillmentOrder"],
            json!([
                {"fulfillmentOrderId":"gid://shopify/FulfillmentOrder/7","fulfillmentOrderLineItems":[{"id":"gid://shopify/FulfillmentOrderLineItem/8","quantity":2}]},
                {"fulfillmentOrderId":"gid://shopify/FulfillmentOrder/17","fulfillmentOrderLineItems":[{"id":"gid://shopify/FulfillmentOrderLineItem/18","quantity":2}]},
            ])
        );
        assert_eq!(requests[reads + 2]["operationName"], "EditOrderTags");
        assert_eq!(
            requests[reads + 2]["variables"]["tags"],
            json!(["SHIPPING"])
        );
    }
}

#[tokio::test]
async fn fulfillment_single_creation_accepts_250_fulfillment_order_entries() {
    let mut order = fulfillable_order(None);
    let template = order["order"]["fulfillmentOrders"]["nodes"][0].clone();
    order["order"]["fulfillmentOrders"]["nodes"]=json!((1..=250).map(|id| {
        let mut node=template.clone();
        node["id"]=json!(format!("gid://shopify/FulfillmentOrder/{id}"));
        node["lineItems"]=page(vec![json!({"id":format!("gid://shopify/FulfillmentOrderLineItem/{id}"),"remainingQuantity":1})],None);
        node
    }).collect::<Vec<_>>());
    let (client,upstream)=client_with_upstream_script(vec![order,json!({"fulfillmentCreate":{"fulfillment":{"id":"gid://shopify/Fulfillment/10"},"userErrors":[]}}),tag_read(42, true), tags_result(true,42)],"http").await;
    let response = client
        .send(fulfill_request(
            "42",
            r#"{"courier":"BEST","tracking_number":"VP123"}"#,
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let requests = upstream.requests();
    assert_eq!(requests.len(), 4);
    let entries = requests[1]["variables"]["fulfillment"]["lineItemsByFulfillmentOrder"]
        .as_array()
        .unwrap();
    assert_eq!(entries.len(), 250);
    let ids: std::collections::HashSet<_> = entries
        .iter()
        .map(|entry| entry["fulfillmentOrderId"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 250);
}

#[tokio::test]
async fn review_plan_reship_resolves_merchant_number_and_returns_acceptance_id() {
    let (url, upstream) = crate::common::scripted_upstream(vec![
        json!({"orders":page(vec![json!({"id":"gid://shopify/Order/42","number":1085,"email":"recipient@example.test"})],None)}),
        tag_read(42, false), tags_result(false,42),
    ], "http").await;
    let mail = ReshipMailServer::start("ok").await;
    let client = reship_client(&url, &mail).await;
    let response = client
        .send(authed(
            "POST",
            "/api/v1/admin/orders/1085/reship",
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        body_json(response).await,
        json!({"success":true,"email_id":"email-fixture-id"})
    );
    let calls = upstream.requests();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0]["operationName"], "ListOrders");
    assert_eq!(calls[0]["variables"]["search"], "name:1085");
    assert_eq!(calls[0]["variables"]["includeContact"], true);
    assert_eq!(calls[2]["variables"]["id"], "gid://shopify/Order/42");
    assert_eq!(mail.requests().len(), 1);
}

#[tokio::test]
async fn review_plan_completed_fulfillment_orders_are_skipped_before_line_reads() {
    let mut order = fulfillable_order(None);
    let mut closed = order["order"]["fulfillmentOrders"]["nodes"][0].clone();
    closed["id"] = json!("gid://shopify/FulfillmentOrder/99");
    closed["status"] = json!("CLOSED");
    order["order"]["fulfillmentOrders"]["nodes"]
        .as_array_mut()
        .unwrap()
        .insert(0, closed);
    let (client,upstream)=client_with_upstream_script(vec![order,json!({"fulfillmentCreate":{"fulfillment":{"id":"gid://shopify/Fulfillment/10"},"userErrors":[]}}),tag_read(42, true), tags_result(true,42)],"http").await;
    let response = client
        .send(fulfill_request(
            "42",
            r#"{"courier":"BEST","tracking_number":"VP123"}"#,
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let calls = upstream.requests();
    assert_eq!(calls.len(), 4);
    assert_eq!(calls[1]["operationName"], "CreateFulfillment");
    assert_eq!(
        calls[1]["variables"]["fulfillment"]["lineItemsByFulfillmentOrder"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn review_plan_listing_without_tag_uses_an_empty_search() {
    let (client, upstream) =
        client_with_upstream_script(vec![json!({"orders":page(vec![],None)})], "http").await;
    let response = client
        .send(authed("GET", "/api/v1/admin/orders", 1, AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(upstream.requests()[0]["variables"]["search"], "");
    assert!(upstream.requests()[0]["variables"]["after"].is_null());
}

#[tokio::test]
async fn reship_uses_one_filtered_query_for_boundary_order_numbers() {
    for number in [1000, 9999] {
        let data = json!({"orders":page(vec![json!({"id":"gid://shopify/Order/42","number":number,"email":"recipient@example.test"})],Some("unused"))});
        let (url, upstream) = crate::common::scripted_upstream(
            vec![data, tag_read(42, false), tags_result(false, 42)],
            "http",
        )
        .await;
        let mail = ReshipMailServer::start("ok").await;
        let client = reship_client(&url, &mail).await;
        let response = client
            .send(authed(
                "POST",
                &format!("/api/v1/admin/orders/{number}/reship"),
                1,
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let calls = upstream.requests();
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls[0]["variables"],
            json!({"first":1,"after":null,"search":format!("name:{number}"),"includeContact":true,"includeTags":false})
        );
        assert_eq!(calls[2]["operationName"], "EditOrderTags");
        assert_eq!(mail.requests().len(), 1);
    }
}

#[tokio::test]
async fn shipping_declaration_markup_accepts_any_selected_price_currency() {
    for (currency, amount, want) in [
        ("USD", "30", 150.0),
        ("EUR", "1", 100.0),
        ("THB", "30", 150.0),
    ] {
        let mut order = shipping_order(1, "US", "USPS");
        order["lineItems"]["nodes"][0]["originalUnitPriceSet"]["shopMoney"] =
            json!({"amount":amount,"currencyCode":currency});
        let (client, upstream) = client_with_upstream_script(
            vec![
                json!({"orders":page(vec![order],None)}),
                tag_read(1, false),
                tags_result(false, 1),
                tag_read(1, true),
                tags_result(true, 1),
            ],
            "http",
        )
        .await;
        let response = client
            .send(authed(
                "POST",
                "/api/v1/admin/orders/shipping_label",
                1,
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            body_json(response).await["fastship"][0]["declared_value"],
            want
        );
        let calls = upstream.requests();
        assert_eq!(calls.len(), 5);
        assert_eq!(calls[2]["variables"]["add"], true);
        assert_eq!(calls[4]["variables"]["add"], false);
    }
}

#[tokio::test]
async fn listing_selection_flags_satisfy_provider_variable_contract_over_tcp() {
    let recorded = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = recorded.clone();
    let router=Router::new().route("/graphql.json",axum::routing::post(move |Json(body):Json<Value>| {
        let captured=captured.clone();
        async move {
            let vars=&body["variables"];
            let valid=vars["includeContact"]==true && vars["includeTags"]==false;
            captured.lock().unwrap().push(body);
            if valid { Json(json!({"data":{"orders":page(vec![],None)}})) }
            else { Json(json!({"errors":[{"message":"Variable selection flag was provided invalid value","extensions":{"code":"INVALID_VARIABLE"}}]})) }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/graphql.json", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let client = crate::common::client_with_shopify(&url).await;
    let response = client
        .send(authed(
            "GET",
            "/api/v1/admin/orders?tag=shipping&fulfillment=UNFULFILLED&page_size=1",
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        body_json(response).await,
        json!({"orders":[],"page_info":{"has_next_page":false,"end_cursor":null}})
    );
    assert_eq!(
        recorded.lock().unwrap()[0]["variables"],
        json!({"first":1,"after":null,"search":"tag:\"shipping\" AND fulfillment_status:unfulfilled","includeContact":true,"includeTags":false})
    );
    task.abort();
}

#[tokio::test]
async fn tag_lookup_unusable_results_stop_each_workflow_over_tcp() {
    for (flow, stage) in [("labels", 1), ("labels", 3), ("fulfill", 2), ("reship", 1)] {
        for (data, status, message) in [
            (
                json!({"orders":page(vec![],None)}),
                404,
                "Shopify order not found",
            ),
            (
                json!({"orders":page(vec![json!({"id":"gid://shopify/Order/99","name":"#99","tags":[]})],None)}),
                502,
                "Upstream request failed",
            ),
            (
                json!({"orders":page(vec![json!({"id":if flow=="labels" {"gid://shopify/Order/1"} else {"gid://shopify/Order/42"},"name":"#42"})],None)}),
                502,
                "Upstream request failed",
            ),
        ] {
            let mut prefix = match flow {
                "labels" => label_prefix(stage),
                "fulfill" => fulfillment_prefix(stage),
                _ => vec![reship_order()],
            };
            prefix.push(data);
            let (url, upstream) = crate::common::scripted_upstream(prefix, "http").await;
            let mail = ReshipMailServer::start("ok").await;
            let client = if flow == "reship" {
                reship_client(&url, &mail).await
            } else {
                crate::common::client_with_shopify(&url).await
            };
            let request = match flow {
                "labels" => authed(
                    "POST",
                    "/api/v1/admin/orders/shipping_label",
                    1,
                    AdminRole::Admin,
                ),
                "fulfill" => fulfill_request(
                    "42",
                    r#"{"courier":"BEST","tracking_number":"VP123"}"#,
                    1,
                    AdminRole::Admin,
                ),
                _ => reship_request(),
            };
            assert_report_error(client.send(request).await.unwrap(), status, message).await;
            let calls = upstream.requests();
            assert_eq!(calls.len(), stage + 1);
            assert_eq!(calls.last().unwrap()["variables"]["includeTags"], true);
            assert_eq!(mail.requests().len(), usize::from(flow == "reship"));
        }
    }
}

#[tokio::test]
async fn case_insensitive_tag_transitions_preserve_workflow_order_over_tcp() {
    let stored = |id: u64, tags: Vec<&str>| json!({"orders":page(vec![json!({"id":format!("gid://shopify/Order/{id}"),"name":"#1","tags":tags})],None)});
    let (client, upstream) = client_with_upstream_script(
        vec![
            json!({"orders":page(vec![shipping_order(1,"US","USPS")],None)}),
            stored(
                1,
                vec!["shipping", "FastShip", "Packing", "packing", "Other", "อื่น"],
            ),
            stored(
                1,
                vec!["shipping", "FastShip", "Packing", "packing", "Other", "อื่น"],
            ),
            tags_result(true, 1),
        ],
        "http",
    )
    .await;
    let response = client
        .send(authed(
            "POST",
            "/api/v1/admin/orders/shipping_label",
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let calls = upstream.requests();
    assert_eq!(calls.len(), 4);
    assert_eq!(calls[3]["variables"]["tags"], json!(["Packing", "packing"]));
    let (client,upstream)=client_with_upstream_script(vec![fulfillable_order(None),json!({"fulfillmentCreate":{"fulfillment":{"id":"gid://shopify/Fulfillment/10"},"userErrors":[]}}),stored(42,vec!["shipping","sHiPpInG","Other"]),tags_result(true,42)],"http").await;
    let response = client
        .send(fulfill_request(
            "42",
            r#"{"courier":"BEST","tracking_number":"VP123"}"#,
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let calls = upstream.requests();
    assert_eq!(calls[1]["operationName"], "CreateFulfillment");
    assert_eq!(
        calls[3]["variables"]["tags"],
        json!(["shipping", "sHiPpInG"])
    );
    let (url, upstream) = crate::common::scripted_upstream(
        vec![reship_order(), stored(42, vec!["returned", "Other"])],
        "http",
    )
    .await;
    let mail = ReshipMailServer::start("ok").await;
    let client = reship_client(&url, &mail).await;
    let response = client.send(reship_request()).await.unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(mail.requests().len(), 1);
    assert_eq!(upstream.requests().len(), 2);
}

#[tokio::test]
async fn order_listing_returns_nullable_customer_contacts_and_preserves_pagination() {
    let nodes = vec![
        json!({"id":"gid://shopify/Order/1","number":1085,"shippingAddress":{"countryCodeV2":"TH"},"email":"order@example.test","customer":{"firstName":" Jane ","lastName":" Doe "}}),
        json!({"id":"gid://shopify/Order/2","number":1086,"shippingAddress":null,"email":"guest@example.test","customer":null}),
        json!({"id":"gid://shopify/Order/3","number":1087,"shippingAddress":{"countryCodeV2":null},"email":null,"customer":{"firstName":null,"lastName":" นามสกุล "}}),
        json!({"id":"gid://shopify/Order/4","number":1088,"shippingAddress":{"countryCodeV2":"US"},"email":null,"customer":{"firstName":" ","lastName":null}}),
    ];
    let (client, upstream) =
        client_with_upstream_script(vec![json!({"orders":page(nodes,Some("next"))})], "http").await;
    let response = client
        .send(authed(
            "GET",
            "/api/v1/admin/orders?tag=shipping&fulfillment=UNFULFILLED&page_size=4&cursor=prev",
            1,
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        body_json(response).await,
        json!({"orders":[
        {"id":1,"number":1085,"shipping_country_code":"TH","full_name":"Jane Doe","email":"order@example.test"},
        {"id":2,"number":1086,"shipping_country_code":null,"full_name":null,"email":"guest@example.test"},
        {"id":3,"number":1087,"shipping_country_code":null,"full_name":"นามสกุล","email":null},
        {"id":4,"number":1088,"shipping_country_code":"US","full_name":null,"email":null}
    ],"page_info":{"has_next_page":true,"end_cursor":"next"}})
    );
    let calls = upstream.requests();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0]["variables"],
        json!({"first":4,"after":"prev","search":"tag:\"shipping\" AND fulfillment_status:unfulfilled","includeContact":true,"includeTags":false})
    );
    assert!(
        calls[0]["query"]
            .as_str()
            .unwrap()
            .contains("customer @include(if: $includeContact)")
    );
}

#[tokio::test]
async fn order_listing_rejects_incomplete_or_malformed_selected_contacts() {
    let valid = json!({"id":"gid://shopify/Order/42","number":1085,"shippingAddress":null,"email":null,"customer":{"firstName":null,"lastName":null}});
    let mut cases = Vec::new();
    for field in ["email", "customer", "shippingAddress"] {
        let mut node = valid.clone();
        node.as_object_mut().unwrap().remove(field);
        cases.push(node);
    }
    for field in ["firstName", "lastName"] {
        let mut node = valid.clone();
        node["customer"].as_object_mut().unwrap().remove(field);
        cases.push(node);
    }
    for (field, value) in [
        ("email", json!(123)),
        ("customer", json!("wrong")),
        ("shippingAddress", json!({"countryCodeV2":123})),
    ] {
        let mut node = valid.clone();
        node[field] = value;
        cases.push(node);
    }
    let mut node = valid;
    node["customer"]["firstName"] = json!(123);
    cases.push(node);
    for node in cases {
        let (client, upstream) =
            client_with_upstream_script(vec![json!({"orders":page(vec![node],None)})], "http")
                .await;
        let response = client
            .send(authed(
                "GET",
                "/api/v1/admin/orders?tag=shipping",
                1,
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_report_error(response, 502, "Upstream request failed").await;
        assert_eq!(upstream.requests().len(), 1);
    }
}

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
    assert_report_auth(&configured, "/v1/admin/inventories/1").await;
    for role in [AdminRole::Admin, AdminRole::SuperAdmin] {
        for uri in [
            "/v1/admin/inventories/bad",
            "/v1/admin/inventories/0",
            "/v1/admin/inventories/000",
            "/v1/admin/inventories/-1",
            "/v1/admin/inventories/1.5",
            "/v1/admin/inventories/1?page_size=0",
            "/v1/admin/inventories/1?page_size=251",
            "/v1/admin/inventories/1?page_size=-1",
            "/v1/admin/inventories/1?page_size=abc",
            "/v1/admin/inventories/1?page_size=65536",
            "/v1/admin/inventories/1?page_size=1&page_size=2",
            "/v1/admin/inventories/1?cursor=",
            "/v1/admin/inventories/1?cursor=%20",
            "/v1/admin/inventories/1?cursor=a&cursor=b",
            "/v1/admin/inventories/1?unexpected=x",
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
        assert_report_unavailable(&client, "/v1/admin/inventories/1", fixture.root_id(), role)
            .await;
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
            "/v1/admin/inventories/1",
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

async fn assert_inventory_failure(admin_id: i64, products: bool, failure: &'static str) {
    let prefix = if products {
        vec![json!({"location":{"id":"gid://shopify/Location/1"}})]
    } else {
        vec![]
    };
    let (client, upstream) = client_with_upstream_script(prefix, failure).await;
    let started = std::time::Instant::now();
    let response = client
        .send(authed(
            "GET",
            "/v1/admin/inventories/1",
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
        "/v1/admin/inventories/1",
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
            let query = request["query"].as_str().unwrap();
            assert!(query.contains("sortKey: INVENTORY_TOTAL"));
            assert!(query.contains("reverse: true"));
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
            .send(authed(
                "GET",
                "/v1/admin/inventories/1",
                fixture.root_id(),
                role,
            ))
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
                "/v1/admin/inventories/1?page_size=50&cursor=products-2",
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
            "/v1/admin/inventories/994",
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
        "/v1/admin/inventories/994?cursor=empty-next",
        "/v1/admin/inventories/995",
        "/v1/admin/inventories/993?page_size=7&cursor=abc%2B%2F%3D",
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
                &format!("/v1/admin/inventories/{id}"),
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

const ENABLE_URI: &str = "/v1/admin/inventories/1/enable";

fn activation_candidate(
    item: u64,
    product: u64,
    active: bool,
    tracked: bool,
    on_hand: i64,
) -> Value {
    json!({"id":format!("level-{item}"),"isActive":active,
        "quantities":[{"name":"on_hand","quantity":on_hand},{"name":"available","quantity":on_hand-1}],
        "item":{"id":format!("gid://shopify/InventoryItem/{item}"),"sku":null,"tracked":tracked,
        "variant":{"id":format!("gid://shopify/ProductVariant/{item}"),"title":"Variant",
        "product":{"id":format!("gid://shopify/Product/{product}"),"title":"Product"}}}})
}

fn discovery(nodes: Vec<Value>, next: Option<&str>) -> Value {
    json!({"location":{"inventoryLevels":page(nodes,next)}})
}

fn location_found() -> Value {
    json!({"location":{"id":"gid://shopify/Location/1"}})
}

fn activation_success() -> Value {
    json!({"inventoryActivate":{"inventoryLevel":{"id":"level","isActive":true,
        "quantities":[{"name":"on_hand","quantity":5},{"name":"available","quantity":4}]},"userErrors":[]}})
}

#[tokio::test]
async fn enable_inventory_collects_all_pages_then_activates_each_unique_eligible_item() {
    let fixture = fixture().await;
    let one = activation_candidate(1, 42, false, true, 5);
    let (client, upstream) = client_with_upstream_script(
        vec![
            location_found(),
            discovery(
                vec![
                    one.clone(),
                    activation_candidate(2, 42, true, true, 5),
                    activation_candidate(3, 42, false, false, 5),
                    activation_candidate(4, 42, false, true, 0),
                    activation_candidate(5, 42, false, true, -1),
                ],
                Some("a"),
            ),
            discovery(vec![], Some("b")),
            discovery(vec![one, activation_candidate(6, 42, false, true, 5)], None),
            activation_success(),
            activation_success(),
        ],
        "http",
    )
    .await;
    let response = client
        .send(authed(
            "POST",
            ENABLE_URI,
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        body_json(response).await,
        json!({"total":2,"success":2,"failed":0})
    );
    let requests = upstream.requests();
    assert_eq!(requests.len(), 6);
    assert_eq!(requests[0]["operationName"], "Location");
    for (i, after) in [Value::Null, json!("a"), json!("b")]
        .into_iter()
        .enumerate()
    {
        assert_eq!(requests[i + 1]["operationName"], "FindInventoryToActivate");
        assert_eq!(
            requests[i + 1]["variables"],
            json!({"first":250,"after":after,"locationId":"gid://shopify/Location/1"})
        );
    }
    for (i, item) in [1, 6].into_iter().enumerate() {
        let request = &requests[i + 4];
        assert_eq!(request["operationName"], "ActivateExistingInventory");
        assert_eq!(
            request["variables"]["inventoryItemId"],
            format!("gid://shopify/InventoryItem/{item}")
        );
        assert_eq!(
            request["variables"]["locationId"],
            "gid://shopify/Location/1"
        );
        assert_eq!(request["variables"]["onHand"], 5);
        assert_eq!(request["variables"]["available"], 4);
        uuid::Uuid::parse_str(request["variables"]["idempotencyKey"].as_str().unwrap()).unwrap();
    }
    assert_ne!(
        requests[4]["variables"]["idempotencyKey"],
        requests[5]["variables"]["idempotencyKey"]
    );
}

#[tokio::test]
async fn enable_inventory_counts_failures_per_variant_and_continues_after_rejection() {
    let fixture = fixture().await;
    let rejected = json!({"inventoryActivate":{"inventoryLevel":null,"userErrors":[{"field":null,"message":"private token and payload"}]}});
    let mut inactive = activation_success();
    inactive["inventoryActivate"]["inventoryLevel"]["isActive"] = json!(false);
    let null = json!({"inventoryActivate":{"inventoryLevel":null,"userErrors":[]}});
    let (client, upstream) = client_with_upstream_script(
        vec![
            location_found(),
            discovery(
                (1..=4)
                    .map(|n| activation_candidate(n, 42, false, true, 5))
                    .collect(),
                None,
            ),
            rejected,
            inactive,
            null,
            activation_success(),
        ],
        "http",
    )
    .await;
    let response = client
        .send(authed(
            "POST",
            ENABLE_URI,
            fixture.root_id(),
            AdminRole::SuperAdmin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        body_json(response).await,
        json!({"total":4,"success":1,"failed":3,"errors":[
        {"product_id":42,"reason":"Inventory activation rejected"},
        {"product_id":42,"reason":"Inventory activation failed"},
        {"product_id":42,"reason":"Inventory activation failed"}]})
    );
    assert_eq!(upstream.requests().len(), 6);
}

#[tokio::test]
async fn enable_inventory_empty_result_and_missing_location() {
    let fixture = fixture().await;
    for (data, status, expected, calls) in [
        (
            vec![location_found(), discovery(vec![], None)],
            200,
            json!({"total":0,"success":0,"failed":0}),
            2,
        ),
        (
            vec![json!({"location":null})],
            404,
            json!({"error":"Shopify location not found"}),
            1,
        ),
        (
            vec![location_found(), json!({"location":null})],
            404,
            json!({"error":"Shopify location not found"}),
            2,
        ),
    ] {
        let (client, upstream) = client_with_upstream_script(data, "http").await;
        let response = client
            .send(authed(
                "POST",
                ENABLE_URI,
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(body_json(response).await, expected);
        assert_eq!(upstream.requests().len(), calls);
    }
}

#[tokio::test]
async fn enable_inventory_rejects_invalid_requests_without_shopify_calls() {
    use axum::{body::Body, http::Request};
    let fixture = fixture().await;
    let (configured, upstream) = client_with_upstream_script(vec![], "http").await;
    for token in [
        None,
        Some("Bearer invalid"),
        Some("Basic invalid"),
        Some("Bearer "),
    ] {
        let mut request = Request::builder().method("POST").uri(ENABLE_URI);
        if let Some(token) = token {
            request = request.header("authorization", token);
        }
        assert_report_error(
            configured
                .send(request.body(Body::empty()).unwrap())
                .await
                .unwrap(),
            401,
            "Invalid or expired token",
        )
        .await;
    }
    let expired = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &json!({"sub":fixture.root_id(),"username":"fixture","role":"ADMIN","exp":1}),
        &jsonwebtoken::EncodingKey::from_secret(&hex::decode(crate::common::JWT_SECRET).unwrap()),
    )
    .unwrap();
    let request = Request::builder()
        .method("POST")
        .uri(ENABLE_URI)
        .header("authorization", format!("Bearer {expired}"))
        .body(Body::empty())
        .unwrap();
    assert_report_error(
        configured.send(request).await.unwrap(),
        401,
        "Invalid or expired token",
    )
    .await;
    for uri in [
        "/v1/admin/inventories/bad/enable",
        "/v1/admin/inventories/0/enable",
        "/v1/admin/inventories/000/enable",
        "/v1/admin/inventories/-1/enable",
        "/v1/admin/inventories/1.5/enable",
        "/v1/admin/inventories/%20/enable",
        "/v1/admin/inventories/1/enable?page_size=1",
        "/v1/admin/inventories/1/enable?cursor=abc",
    ] {
        assert_report_error(
            configured
                .send(authed("POST", uri, fixture.root_id(), AdminRole::Admin))
                .await
                .unwrap(),
            400,
            "Expected a positive location id and no query parameters",
        )
        .await;
    }
    assert_report_error(
        configured
            .send(authed(
                "POST",
                "/v1/admin/inventories//enable",
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap(),
        400,
        "Expected a positive location id and no query parameters",
    )
    .await;
    assert!(upstream.requests().is_empty());
    for client in [
        client().await,
        crate::common::client_with_shopify("invalid-url").await,
    ] {
        assert_report_error(
            client
                .send(authed(
                    "POST",
                    ENABLE_URI,
                    fixture.root_id(),
                    AdminRole::Admin,
                ))
                .await
                .unwrap(),
            503,
            "Shopify inventory activation unavailable",
        )
        .await;
    }
}

#[tokio::test]
async fn enable_inventory_cookie_auth_obeys_origin_guard() {
    use axum::{body::Body, http::Request};
    let fixture = fixture().await;
    let login =
        crate::v1::auth::cookie_login(&fixture.username("root"), "fixture-root-password").await;
    assert_eq!(login.status(), 200);
    let cookie = crate::v1::auth::cookie_pair(&login);
    let (client, upstream) =
        client_with_upstream_script(vec![location_found(), discovery(vec![], None)], "http").await;
    for origin in [
        None,
        Some("null"),
        Some("https://foreign.example.test"),
        Some("http://127.0.0.1:8084"),
    ] {
        let mut builder = Request::builder()
            .method("POST")
            .uri(ENABLE_URI)
            .header("cookie", &cookie);
        if let Some(origin) = origin {
            builder = builder.header("origin", origin);
        }
        let response = client
            .send(builder.body(Body::empty()).unwrap())
            .await
            .unwrap();
        if origin == Some("http://127.0.0.1:8084") {
            assert_eq!(response.status(), 200);
            assert_eq!(
                body_json(response).await,
                json!({"total":0,"success":0,"failed":0})
            );
        } else {
            assert_report_error(
                response,
                403,
                "Your account is forbidden to make this action",
            )
            .await;
            assert!(upstream.requests().is_empty());
        }
    }
    assert_eq!(upstream.requests().len(), 2);
}

#[tokio::test]
async fn enable_inventory_discovery_errors_never_activate_partial_candidates() {
    let fixture = fixture().await;
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
            let mut prefix = vec![];
            if stage > 0 {
                prefix.push(location_found());
            }
            if stage > 1 {
                prefix.push(discovery(
                    vec![activation_candidate(1, 42, false, true, 5)],
                    Some("next"),
                ));
            }
            let (client, upstream) = client_with_upstream_script(prefix, failure).await;
            let response = client
                .send(authed(
                    "POST",
                    ENABLE_URI,
                    fixture.root_id(),
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
            let requests = upstream.requests();
            assert_eq!(
                requests.len(),
                stage + if failure == "throttled" { 4 } else { 1 }
            );
            assert!(
                requests
                    .iter()
                    .all(|r| r["operationName"] != "ActivateExistingInventory")
            );
        }
    }
    let client = client_with_unreachable_shopify().await;
    assert_report_error(
        client
            .send(authed(
                "POST",
                ENABLE_URI,
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap(),
        502,
        "Upstream request failed",
    )
    .await;
}

#[tokio::test]
async fn enable_inventory_invalid_discovery_data_fails_before_any_activation() {
    let fixture = fixture().await;
    let candidate = activation_candidate(1, 42, false, true, 5);
    let mut cases = vec![vec![json!({})]];
    for cursor in [Value::Null, json!(""), json!(" ")] {
        cases.push(vec![json!({"location":{"inventoryLevels":{"nodes":[candidate.clone()],"pageInfo":{"hasNextPage":true,"endCursor":cursor}}}})]);
    }
    cases.push(vec![
        discovery(vec![candidate.clone()], Some("a")),
        discovery(vec![], Some("a")),
    ]);
    cases.push(vec![
        discovery(vec![candidate.clone()], Some("a")),
        discovery(vec![], Some("b")),
        discovery(vec![], Some("a")),
    ]);
    for field in [
        "missing_quantity",
        "missing_on_hand",
        "overflow_available",
        "zero_item",
        "zero_variant",
        "duplicate_quantity",
        "overflow_quantity",
        "bad_product",
        "overflow_product",
        "bad_item",
    ] {
        let mut bad = activation_candidate(2, 43, false, true, 5);
        match field {
            "missing_on_hand" => bad["quantities"] = json!([{"name":"available","quantity":4}]),
            "overflow_available" => bad["quantities"][1]["quantity"] = json!(i64::MAX),
            "zero_item" => bad["item"]["id"] = json!("gid://shopify/InventoryItem/0"),
            "zero_variant" => {
                bad["item"]["variant"]["id"] = json!("gid://shopify/ProductVariant/0")
            }
            "missing_quantity" => bad["quantities"] = json!([{"name":"on_hand","quantity":5}]),
            "duplicate_quantity" => {
                bad["quantities"] = json!([{"name":"on_hand","quantity":5},{"name":"on_hand","quantity":6},{"name":"available","quantity":4}])
            }
            "overflow_quantity" => bad["quantities"][0]["quantity"] = json!(i64::MAX),
            "bad_product" => {
                bad["item"]["variant"]["product"]["id"] = json!("gid://shopify/Location/42")
            }
            "overflow_product" => {
                bad["item"]["variant"]["product"]["id"] =
                    json!("gid://shopify/Product/18446744073709551616")
            }
            "bad_item" => bad["item"]["id"] = json!("gid://shopify/Product/1"),
            _ => unreachable!(),
        }
        cases.push(vec![discovery(vec![candidate.clone(), bad], None)]);
    }
    let mut conflicting = candidate.clone();
    conflicting["quantities"][0]["quantity"] = json!(8);
    cases.push(vec![
        discovery(vec![candidate.clone()], Some("a")),
        discovery(vec![conflicting], None),
    ]);
    for mut data in cases {
        data.insert(0, location_found());
        let expected = data.len();
        let (client, upstream) = client_with_upstream_script(data, "http").await;
        assert_report_error(
            client
                .send(authed(
                    "POST",
                    ENABLE_URI,
                    fixture.root_id(),
                    AdminRole::Admin,
                ))
                .await
                .unwrap(),
            502,
            "Upstream request failed",
        )
        .await;
        assert_eq!(upstream.requests().len(), expected);
        assert!(
            upstream
                .requests()
                .iter()
                .all(|r| r["operationName"] != "ActivateExistingInventory")
        );
    }
}

#[tokio::test]
async fn enable_inventory_mutation_dependency_failures_are_counted_and_do_not_stop_batch() {
    let fixture = fixture().await;
    for succeeded in [0, 1] {
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
            let mut prefix = vec![
                location_found(),
                discovery(
                    (1..=3)
                        .map(|n| activation_candidate(n, 42, false, true, 5))
                        .collect(),
                    None,
                ),
            ];
            if succeeded == 1 {
                prefix.push(activation_success());
            }
            let (client, upstream) = client_with_upstream_script(prefix, failure).await;
            let response = client
                .send(authed(
                    "POST",
                    ENABLE_URI,
                    fixture.root_id(),
                    AdminRole::Admin,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), 200);
            let body = body_json(response).await;
            assert_eq!(body["total"], 3);
            assert_eq!(body["success"], succeeded);
            assert_eq!(body["failed"], 3 - succeeded);
            let errors = body["errors"].as_array().unwrap();
            assert_eq!(errors.len(), 3 - succeeded);
            for error in errors {
                assert_eq!(
                    *error,
                    json!({"product_id":42,"reason":if failure=="timeout" {"Inventory activation timed out; outcome could not be confirmed"}else{"Inventory activation failed"}})
                );
            }
            let requests = upstream.requests();
            assert_eq!(
                requests.len(),
                2 + succeeded + (3 - succeeded) * if failure == "throttled" { 4 } else { 1 }
            );
            let mutations = &requests[2..];
            for item in 1..=3 {
                let attempts = mutations
                    .iter()
                    .filter(|r| {
                        r["variables"]["inventoryItemId"]
                            == format!("gid://shopify/InventoryItem/{item}")
                    })
                    .collect::<Vec<_>>();
                assert!(!attempts.is_empty());
                assert!(
                    attempts
                        .iter()
                        .all(|r| r["variables"] == attempts[0]["variables"])
                );
            }
        }
    }
}

struct EnableWireServer {
    task: tokio::task::JoinHandle<()>,
    requests: std::sync::Arc<std::sync::Mutex<Vec<Value>>>,
}
impl Drop for EnableWireServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn enable_wire_server(
    replies: Vec<Option<Value>>,
) -> (crate::common::TestClient, EnableWireServer) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = requests.clone();
    let task = tokio::spawn(async move {
        let mut replies = std::collections::VecDeque::from(replies);
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let (header_end, length) = loop {
                let mut buffer = [0_u8; 4096];
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                    assert!(headers.contains("x-shopify-access-token: e2e-fixture-token"));
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|n| n.parse::<usize>().unwrap())
                        })
                        .unwrap();
                    break (end + 4, length);
                }
            };
            while bytes.len() < header_end + length {
                let mut buffer = [0_u8; 4096];
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
            }
            recorded
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap());
            if let Some(reply) = replies
                .pop_front()
                .expect("unexpected extra Shopify request")
            {
                let body = reply.to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        }
    });
    let client =
        crate::common::client_with_shopify(&format!("http://{address}/graphql.json")).await;
    (client, EnableWireServer { task, requests })
}

#[tokio::test]
async fn enable_inventory_connection_loss_is_safe_at_every_stage() {
    let fixture = fixture().await;
    for stage in 0..4 {
        let mut replies = vec![];
        if stage > 0 {
            replies.push(Some(json!({"data":location_found()})));
        }
        if stage > 1 {
            replies.push(Some(json!({"data":discovery(vec![activation_candidate(1,42,false,true,5),activation_candidate(2,42,false,true,5)],if stage==2 {Some("next")}else{None})})));
        }
        replies.push(None);
        if stage == 3 {
            replies.push(Some(json!({"data":activation_success()})));
        }
        let (client, upstream) = enable_wire_server(replies).await;
        let response = client
            .send(authed(
                "POST",
                ENABLE_URI,
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        if stage < 3 {
            assert_report_error(response, 502, "Upstream request failed").await;
            assert!(
                upstream
                    .requests
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|r| r["operationName"] != "ActivateExistingInventory")
            );
        } else {
            assert_eq!(response.status(), 200);
            assert_eq!(
                body_json(response).await,
                json!({"total":2,"success":1,"failed":1,"errors":[{"product_id":42,"reason":"Inventory activation failed"}]})
            );
        }
        assert_eq!(
            upstream.requests.lock().unwrap().len(),
            if stage == 3 { 4 } else { stage + 1 }
        );
    }
}

#[tokio::test]
async fn enable_inventory_throttle_retry_preserves_quantities_and_uuid_then_succeeds() {
    let fixture = fixture().await;
    let (client, upstream) = enable_wire_server(vec![
        Some(json!({"data":location_found()})),
        Some(json!({"data":discovery(vec![activation_candidate(1,42,false,true,5)],None)})),
        Some(json!({"errors":[{"message":"private throttle","extensions":{"code":"THROTTLED"}}]})),
        Some(json!({"data":activation_success()})),
    ])
    .await;
    let response = client
        .send(authed(
            "POST",
            ENABLE_URI,
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        body_json(response).await,
        json!({"total":1,"success":1,"failed":0})
    );
    let requests = upstream.requests.lock().unwrap();
    assert_eq!(requests.len(), 4);
    assert_eq!(requests[2], requests[3]);
    assert_eq!(requests[2]["variables"]["onHand"], 5);
    assert_eq!(requests[2]["variables"]["available"], 4);
}

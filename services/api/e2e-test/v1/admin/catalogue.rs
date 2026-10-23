use crate::common::{
    assert_report_auth, assert_report_error, assert_report_unavailable, authed, body_json, client,
    client_with_unreachable_shopify, client_with_upstream_script, fixture, shopify_page as page,
};
use domain::dto::AdminRole;
use serde_json::{Value, json};

const URI: &str = "/api/v1/admin/conventions/catalogue?location_id=gid://shopify/Location/1";
const INVALID: &str = "Expected a Location GID, page_size between 1 and 250, and nonblank cursor/search up to 2048 bytes";
fn location() -> Value {
    json!({"location":{"id":"gid://shopify/Location/1"}})
}
fn variant() -> Value {
    json!({"id":"gid://shopify/ProductVariant/2","title":"Large","sku":"SKU-2","image":null,"selectedOptions":[{"name":"Size","value":"Large"}],"inventoryPolicy":"DENY","contextualPricing":{"price":{"amount":"123.4500","currencyCode":"THB"}},"inventoryItem":{"id":"gid://shopify/InventoryItem/3","tracked":true,"inventoryLevel":{"id":"gid://shopify/InventoryLevel/4","isActive":true,"quantities":[{"name":"available","quantity":0}]}}})
}
fn product(variants: Vec<Value>) -> Value {
    json!({"id":"gid://shopify/Product/1","title":"Shirt","description":"A cotton shirt","featuredImage":{"url":"https://example.test/shirt.jpg"},"variants":page(variants,None)})
}
fn products(nodes: Vec<Value>, next: Option<&str>) -> Value {
    json!({"products":page(nodes,next)})
}

#[tokio::test]
async fn catalogue_preserves_content_zero_and_forwards_raw_search_and_cursor() {
    let fixture = fixture().await;
    let mut missing = variant();
    missing["inventoryItem"]["inventoryLevel"] = Value::Null;
    let mut inactive = variant();
    inactive["inventoryItem"]["inventoryLevel"]["isActive"] = json!(false);
    let mut untracked = variant();
    untracked["inventoryItem"]["tracked"] = json!(false);
    let mut continuing = variant();
    continuing["inventoryPolicy"] = json!("CONTINUE");
    let mut only_untracked = variant();
    only_untracked["inventoryItem"]["tracked"] = json!(false);
    let (client, upstream) = client_with_upstream_script(
        vec![
            location(),
            products(
                vec![
                    product(vec![
                        variant(),
                        missing.clone(),
                        inactive.clone(),
                        untracked,
                        continuing,
                    ]),
                    product(vec![missing.clone(), inactive.clone()]),
                    product(vec![only_untracked]),
                ],
                Some("next"),
            ),
            location(),
            products(
                vec![product(vec![missing, inactive])],
                Some("filtered-next"),
            ),
        ],
        "http",
    )
    .await;
    let response = client
        .send(authed(
            "GET",
            &format!(
                "{URI}&search=collection_id%3A123%20OR%20title%3Ahead*&page_size=1&cursor=opaque"
            ),
            fixture.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = body_json(response).await;
    assert_eq!(
        body["page_info"],
        json!({"has_next_page":true,"end_cursor":"next"})
    );
    let p = &body["products"][0];
    assert!(p.get("status").is_none());
    assert_eq!(p["id"], "gid://shopify/Product/1");
    assert_eq!(p["description"], "A cotton shirt");
    assert_eq!(p["featured_image"]["url"], "https://example.test/shirt.jpg");
    let v = &p["variants"][0];
    assert_eq!(
        *v,
        json!({"id":"gid://shopify/ProductVariant/2","inventory_item_id":"gid://shopify/InventoryItem/3","title":"Large","sku":"SKU-2","image":null,"selected_options":[{"name":"Size","value":"Large"}],"price":{"amount":"123.4500","currency_code":"THB"},"tracked":true,"inventory_policy":"DENY","available_quantity":0,"eligible":true})
    );
    assert_eq!(p["variants"][1]["inventory_policy"], "CONTINUE");
    assert_eq!(p["variants"][1]["eligible"], true);
    assert!(p["variants"][1].get("unavailable_reason").is_none());
    assert_eq!(p["variants"][1]["available_quantity"], 0);
    assert_eq!(
        p["featured_image"],
        json!({"url":"https://example.test/shirt.jpg"})
    );
    assert_eq!(body["products"].as_array().unwrap().len(), 1);
    assert_eq!(p["variants"].as_array().unwrap().len(), 2);
    assert!(!body.to_string().contains("unitCost"));
    let response = client
        .send(authed("GET", URI, fixture.root_id(), AdminRole::SuperAdmin))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let empty = body_json(response).await;
    assert_eq!(empty["products"], json!([]));
    assert_eq!(
        empty["page_info"],
        json!({"has_next_page":true,"end_cursor":"filtered-next"})
    );
    let requests = upstream.requests();
    assert_eq!(requests.len(), 4);
    let query = requests[1]["query"].as_str().unwrap();
    assert!(!query.contains("altText"));
    assert!(!query.lines().any(|line| line.trim() == "status"));
    assert_eq!(
        requests[1]["variables"],
        json!({"locationId":"gid://shopify/Location/1","first":1,"after":"opaque","search":"status:active AND tracks_inventory:true AND (collection_id:123 OR title:head*)"})
    );
    assert_eq!(
        requests[3]["variables"],
        json!({"locationId":"gid://shopify/Location/1","first":25,"after":null,"search":"status:active AND tracks_inventory:true"})
    );
}

#[tokio::test]
async fn catalogue_validation_authentication_and_unavailable_clients() {
    let fixture = fixture().await;
    let (configured, upstream) = client_with_upstream_script(vec![], "http").await;
    assert_report_auth(&configured, URI).await;
    let expired = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &json!({"sub":fixture.root_id(),"username":"fixture","role":"ADMIN","exp":1}),
        &jsonwebtoken::EncodingKey::from_secret(&hex::decode(crate::common::JWT_SECRET).unwrap()),
    )
    .unwrap();
    let request = axum::http::Request::builder()
        .uri(URI)
        .header("authorization", format!("Bearer {expired}"))
        .body(axum::body::Body::empty())
        .unwrap();
    assert_report_error(
        configured.send(request).await.unwrap(),
        401,
        "Invalid or expired token",
    )
    .await;
    let mut invalid = vec!["/api/v1/admin/conventions/catalogue".to_owned()];
    for location in [
        "",
        "1",
        "gid://shopify/Product/1",
        "gid://shopify/Location/0",
        "gid://shopify/Location/abc",
        "gid://shopify/Location/18446744073709551616",
    ] {
        invalid.push(format!(
            "/api/v1/admin/conventions/catalogue?location_id={location}"
        ));
    }
    for query in [
        "page_size=0",
        "page_size=251",
        "page_size=-1",
        "page_size=abc",
        "page_size=65536",
        "page_size=1&page_size=2",
        "cursor=",
        "cursor=%20",
        "cursor=a&cursor=b",
        "search=",
        "search=%20",
        "search=a&search=b",
        "unexpected=x",
        "location_id=gid://shopify/Location/2",
    ] {
        invalid.push(format!("{URI}&{query}"));
    }
    for field in ["cursor", "search"] {
        invalid.push(format!("{URI}&{field}={}", "x".repeat(2049)));
        invalid.push(format!("{URI}&{field}={}", "%E0%B8%81".repeat(683)));
    }
    for uri in invalid {
        assert_report_error(
            configured
                .send(authed("GET", &uri, fixture.root_id(), AdminRole::Admin))
                .await
                .unwrap(),
            400,
            INVALID,
        )
        .await;
    }
    assert_eq!(upstream.requests(), [] as [Value; 0]);
    for c in [
        client().await,
        crate::common::client_with_shopify("invalid-url").await,
    ] {
        for role in [AdminRole::Admin, AdminRole::SuperAdmin] {
            assert_report_unavailable(&c, URI, fixture.root_id(), role).await;
        }
    }
}

#[tokio::test]
async fn catalogue_rejects_incomplete_nested_variants_without_returning_partial_products() {
    let fixture = fixture().await;
    let mut overflow = product(vec![variant(); 250]);
    overflow["variants"]["pageInfo"] = json!({"hasNextPage":true,"endCursor":"nested"});
    let (client, upstream) = client_with_upstream_script(
        vec![
            location(),
            products(vec![product(vec![variant()]), overflow], None),
        ],
        "http",
    )
    .await;
    assert_report_error(
        client
            .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
            .await
            .unwrap(),
        502,
        "Upstream request failed",
    )
    .await;
    assert_eq!(upstream.requests().len(), 2);
}

#[tokio::test]
async fn catalogue_fails_whole_page_for_incomplete_or_invalid_provider_values() {
    let fixture = fixture().await;
    let mut cases = Vec::new();
    for path in [
        "id",
        "contextualPricing",
        "inventoryItem",
        "selectedOptions",
        "inventoryPolicy",
    ] {
        let mut v = variant();
        v.as_object_mut().unwrap().remove(path);
        cases.push(products(vec![product(vec![variant(), v])], None));
    }
    for field in ["amount", "currencyCode"] {
        let mut v = variant();
        v["contextualPricing"]["price"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        cases.push(products(vec![product(vec![v])], None));
        let mut v = variant();
        v["contextualPricing"]["price"][field] = json!("");
        cases.push(products(vec![product(vec![v])], None));
    }
    for id in ["gid://shopify/Product/2", "gid://shopify/ProductVariant/0"] {
        let mut v = variant();
        v["id"] = json!(id);
        cases.push(products(vec![product(vec![v])], None));
    }
    for quantities in [
        json!([]),
        json!([{"name":"on_hand","quantity":7}]),
        json!([{"name":"available","quantity":1},{"name":"available","quantity":2}]),
    ] {
        for tracked in [true, false] {
            let mut v = variant();
            v["inventoryItem"]["tracked"] = json!(tracked);
            v["inventoryItem"]["inventoryLevel"]["quantities"] = quantities.clone();
            cases.push(products(vec![product(vec![v])], None));
        }
    }
    let mut missing = products(vec![], None);
    missing["products"]
        .as_object_mut()
        .unwrap()
        .remove("pageInfo");
    cases.push(missing);
    cases.push(json!({"products":{"nodes":[],"pageInfo":{"hasNextPage":true,"endCursor":null}}}));
    for field in ["id", "title", "description", "featuredImage"] {
        let mut p = product(vec![variant()]);
        p.as_object_mut().unwrap().remove(field);
        cases.push(products(vec![p], None));
    }
    for id in ["gid://shopify/Location/1", "gid://shopify/Product/0"] {
        let mut p = product(vec![variant()]);
        p["id"] = json!(id);
        cases.push(products(vec![p], None));
    }
    for field in ["id", "tracked", "inventoryLevel"] {
        let mut v = variant();
        v["inventoryItem"].as_object_mut().unwrap().remove(field);
        cases.push(products(vec![product(vec![v])], None));
    }
    for amount in ["NaN", "not-a-price"] {
        let mut v = variant();
        v["inventoryItem"]["inventoryLevel"] = Value::Null;
        v["contextualPricing"]["price"]["amount"] = json!(amount);
        cases.push(products(vec![product(vec![v])], None));
    }
    for cursor in [Value::Null, json!(""), json!(" ")] {
        cases.push(
            json!({"products":{"nodes":[],"pageInfo":{"hasNextPage":true,"endCursor":cursor}}}),
        );
    }
    for nested in [false, true] {
        let mut data = products(vec![product(vec![variant()])], None);
        let page = if nested {
            &mut data["products"]["nodes"][0]["variants"]["pageInfo"]
        } else {
            &mut data["products"]["pageInfo"]
        };
        page.as_object_mut().unwrap().remove("endCursor");
        cases.push(data);
    }
    for data in cases {
        let (client, upstream) = client_with_upstream_script(vec![location(), data], "http").await;
        assert_report_error(
            client
                .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
                .await
                .unwrap(),
            502,
            "Upstream request failed",
        )
        .await;
        assert_eq!(upstream.requests().len(), 2);
    }
}

#[tokio::test]
async fn catalogue_missing_location_is_not_found_before_browsing() {
    let fixture = fixture().await;
    let (client, upstream) =
        client_with_upstream_script(vec![json!({"location":null})], "http").await;
    assert_report_error(
        client
            .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
            .await
            .unwrap(),
        404,
        "Shopify location not found",
    )
    .await;
    assert_eq!(upstream.requests().len(), 1);
}

#[tokio::test]
async fn catalogue_dependency_failures_at_location_browse_and_continuation() {
    let fixture = fixture().await;
    for browse in [false, true] {
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
            let prefix = if browse { vec![location()] } else { vec![] };
            let (client, upstream) = client_with_upstream_script(prefix, failure).await;
            assert_report_error(
                client
                    .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
                    .await
                    .unwrap(),
                if failure == "timeout" { 504 } else { 502 },
                "Upstream request failed",
            )
            .await;
            assert_eq!(
                upstream.requests().len(),
                usize::from(browse) + if failure == "throttled" { 4 } else { 1 }
            );
        }
    }
    let client = client_with_unreachable_shopify().await;
    assert_report_error(
        client
            .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
            .await
            .unwrap(),
        502,
        "Upstream request failed",
    )
    .await;
    let (client, upstream) = client_with_upstream_script(
        vec![location(), products(vec![], Some("next")), location()],
        "http",
    )
    .await;
    assert_eq!(
        client
            .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
            .await
            .unwrap()
            .status(),
        200
    );
    assert_report_error(
        client
            .send(authed(
                "GET",
                &format!("{URI}&cursor=next"),
                fixture.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap(),
        502,
        "Upstream request failed",
    )
    .await;
    assert_eq!(upstream.requests()[3]["variables"]["after"], "next");
}

#[tokio::test]
async fn catalogue_accepts_maximum_boundaries_and_opaque_search_without_normalizing() {
    let fixture = fixture().await;
    let boundary = "x".repeat(2048);
    let multibyte = format!("{}x", "ก".repeat(682));
    for (size, search, cursor) in [
        (250, boundary.as_str(), boundary.as_str()),
        (1, "title:head*", "null"),
        (25, "collection_id:123", "abc"),
        (25, multibyte.as_str(), "abc"),
    ] {
        let (client, upstream) =
            client_with_upstream_script(vec![location(), products(vec![], None)], "http").await;
        let query = reqwest::Url::parse_with_params(
            "http://example.test/api/v1/admin/conventions/catalogue",
            [
                ("location_id", "gid://shopify/Location/1"),
                ("page_size", &size.to_string()),
                ("search", search),
                ("cursor", cursor),
            ],
        )
        .unwrap();
        let uri = format!("{}?{}", query.path(), query.query().unwrap());
        assert_eq!(
            client
                .send(authed("GET", &uri, fixture.root_id(), AdminRole::Admin))
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            upstream.requests()[1]["variables"],
            json!({"locationId":"gid://shopify/Location/1","first":size,"after":cursor,"search":format!("status:active AND tracks_inventory:true AND ({search})")})
        );
    }
}

#[tokio::test]
async fn catalogue_connection_loss_at_both_upstream_operations_is_not_success() {
    let fixture = fixture().await;
    for browse in [false, true] {
        let mut replies = Vec::new();
        if browse {
            replies.push(Some(json!({"data":location()})));
        }
        replies.push(None);
        let (client, upstream) = crate::common::client_with_wire_script(replies).await;
        assert_report_error(
            client
                .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
                .await
                .unwrap(),
            502,
            "Upstream request failed",
        )
        .await;
        assert_eq!(
            upstream.requests.lock().unwrap().len(),
            usize::from(browse) + 1
        );
    }
}

#[tokio::test]
async fn catalogue_invalid_location_response_is_upstream_failure_before_browsing() {
    let fixture = fixture().await;
    for location in [
        json!({}),
        json!({"location":{}}),
        json!({"location":{"id":"gid://shopify/Location/2"}}),
        json!({"location":{"id":"gid://shopify/Product/1"}}),
    ] {
        let (client, upstream) = client_with_upstream_script(vec![location], "http").await;
        assert_report_error(
            client
                .send(authed("GET", URI, fixture.root_id(), AdminRole::Admin))
                .await
                .unwrap(),
            502,
            "Upstream request failed",
        )
        .await;
        assert_eq!(upstream.requests().len(), 1);
    }
}

#[tokio::test]
async fn catalogue_old_path_is_absent() {
    let f = fixture().await;
    let c = client().await;
    assert_eq!(
        c.send(authed(
            "GET",
            "/api/v1/admin/catalogue/products?location_id=gid://shopify/Location/1",
            f.root_id(),
            AdminRole::Admin
        ))
        .await
        .unwrap()
        .status(),
        404
    );
}

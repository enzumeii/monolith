#![allow(clippy::too_many_lines)]
use crate::common::{authed, authed_json, body_json, client, client_with_upstream_script, fixture};
use domain::dto::AdminRole;
use serde_json::{Value, json};
const URI: &str = "/api/v1/admin/conventions";
fn input(slug: &str) -> Value {
    json!({"slug":slug,"name":"Convention","country_code":"th","event_starts_on":"2028-02-29","event_ends_on":"2028-02-29","description_markdown":"# Event\nWelcome","currency_code":"THB","payment_methods":["stripe","paypal"]})
}
#[tokio::test]
async fn conventions_create_disabled_and_read_exact_metadata() {
    let fixture = fixture().await;
    let client = client().await;
    let response = client
        .send(authed_json(
            "POST",
            URI,
            fixture.root_id(),
            AdminRole::Admin,
            &input(fixture.prefix()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 201);
    let body = body_json(response).await;
    assert_eq!(body["preorder_enabled"], false);
    assert_eq!(body["country_code"], "TH");
    assert_eq!(body["event_starts_on"], "2028-02-29");
    assert_eq!(body["description_markdown"], "# Event\nWelcome");
}

fn item(number: u64, price: &str) -> Value {
    json!({"shopify_product_id":"gid://shopify/Product/1","shopify_variant_id":format!("gid://shopify/ProductVariant/{number}"),"final_price":price})
}
fn remote(number: u64) -> Value {
    json!({"productVariant":{"id":format!("gid://shopify/ProductVariant/{number}"),"product":{"id":"gid://shopify/Product/1","status":"ACTIVE"},"inventoryPolicy":"DENY","inventoryItem":{"tracked":true}}})
}
async fn create(
    c: &crate::common::TestClient,
    f: &crate::common::TestFixture,
    value: Value,
) -> Value {
    let r = c
        .send(authed_json(
            "POST",
            URI,
            f.root_id(),
            AdminRole::Admin,
            &value,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), 201, "{}", body_json(r).await);
    body_json(r).await
}
async fn mutate(
    c: &crate::common::TestClient,
    f: &crate::common::TestFixture,
    method: &str,
    path: &str,
    value: Value,
    status: u16,
) -> Value {
    let r = c
        .send(authed_json(
            method,
            path,
            f.root_id(),
            AdminRole::SuperAdmin,
            &value,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), status, "{}", body_json(r).await);
    body_json(r).await
}
#[tokio::test]
async fn conventions_lifecycle_exact_prices_replacement_and_cascade() {
    let f = fixture().await;
    let (c, upstream) =
        client_with_upstream_script(vec![remote(2), remote(3), remote(2), remote(2)], "http").await;
    let created = create(&c, &f, input(f.prefix())).await;
    let id = created["id"].as_str().unwrap();
    let path = format!("{URI}/{id}");
    let variants = format!("{path}/variants");
    let first = mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"12.99"),item(3,"20")]}),
        200,
    )
    .await;
    let selected = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["shopify_variant_id"] == "gid://shopify/ProductVariant/2")
        .unwrap();
    assert_eq!(selected["final_price"], "12.99");
    let row = selected["id"].clone();
    let amount: String=sqlx::query_scalar("SELECT final_price::text FROM convention_variants WHERE convention_id=$1 AND shopify_variant_id=$2").bind(uuid::Uuid::parse_str(id).unwrap()).bind("gid://shopify/ProductVariant/2").fetch_one(&f.pool()).await.unwrap();
    assert_eq!(amount, "12.99");
    let second = mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"15")]}),
        200,
    )
    .await;
    assert_eq!(second["items"][0]["id"], row);
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["final_price"], "15.00");
    let r = c
        .send(authed(
            "GET",
            &format!("{variants}?page=1&per_page=1"),
            f.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(body_json(r).await["items"], second["items"]);
    let enabled = mutate(
        &c,
        &f,
        "PATCH",
        &path,
        json!({"preorder_enabled":true}),
        200,
    )
    .await;
    assert_eq!(enabled["preorder_enabled"], true);
    mutate(&c, &f, "PUT", &variants, json!({"items":[]}), 409).await;
    mutate(
        &c,
        &f,
        "PATCH",
        &path,
        json!({"preorder_enabled":false,"logo_url":"https://example.test/logo.png"}),
        200,
    )
    .await;
    let cleared = mutate(
        &c,
        &f,
        "PATCH",
        &path,
        json!({"logo_url":null,"name":"  Updated  "}),
        200,
    )
    .await;
    assert!(cleared["logo_url"].is_null());
    assert_eq!(cleared["name"], "Updated");
    mutate(&c, &f, "PUT", &variants, json!({"items":[]}), 200).await;
    mutate(
        &c,
        &f,
        "PATCH",
        &path,
        json!({"preorder_enabled":true}),
        409,
    )
    .await;
    let r = c
        .send(authed(
            "GET",
            &format!("{URI}?preorder_enabled=false"),
            f.root_id(),
            AdminRole::Admin,
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let list = body_json(r).await;
    assert_eq!(list["page"], 1);
    assert_eq!(list["per_page"], 25);
    assert!(
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == id)
    );
    for statement in [
        "UPDATE conventions SET slug='changed' WHERE id=$1",
        "UPDATE conventions SET currency_code='SGD' WHERE id=$1",
    ] {
        assert!(
            sqlx::query(statement)
                .bind(uuid::Uuid::parse_str(id).unwrap())
                .execute(&f.pool())
                .await
                .is_err()
        );
    }
    let r = c
        .send(authed("DELETE", &path, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(r.status(), 204);
    let r = c
        .send(authed("GET", &path, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(r.status(), 404);
    assert_eq!(upstream.requests().len(), 4);
}
#[tokio::test]
async fn conventions_validation_families_reject_without_shopify_calls() {
    let f = fixture().await;
    let (c, upstream) = client_with_upstream_script(vec![], "http").await;
    let base = input(f.prefix());
    for field in [
        "slug",
        "name",
        "country_code",
        "event_starts_on",
        "event_ends_on",
        "description_markdown",
        "currency_code",
        "payment_methods",
    ] {
        for missing in [true, false] {
            let mut v = base.clone();
            if missing {
                v.as_object_mut().unwrap().remove(field);
            } else {
                v[field] = Value::Null;
            }
            mutate(&c, &f, "POST", URI, v, 400).await;
        }
    }
    let mut cases = vec![];
    for slug in [
        "",
        "A",
        "a--b",
        "-a",
        "a-",
        "a_b",
        "api",
        "v1",
        "healthz",
        "backoffice",
        "user",
        "assets",
    ] {
        cases.push(("slug", json!(slug)));
    }
    cases.push(("slug", json!("x".repeat(64))));
    for name in [String::new(), " ".to_owned(), "ก".repeat(121)] {
        cases.push(("name", json!(name)));
    }
    for country in ["ZZ", "THA", "123", "T"] {
        cases.push(("country_code", json!(country)));
    }
    for date in [
        "2027-02-29",
        "2028-2-29",
        "2028-02-30",
        "2028-02-29T00:00:00Z",
    ] {
        cases.push(("event_starts_on", json!(date)));
    }
    cases.push(("event_ends_on", json!("2028-02-28")));
    for markdown in [" ".to_owned(), "x\0".to_owned(), "ก".repeat(6667)] {
        cases.push(("description_markdown", json!(markdown)));
    }
    for logo in [
        "http://example.test/a",
        "/a",
        "https://u:p@example.test/a",
        "https://example.test/a#f",
        "https://",
        "https://example.test/".to_owned().as_str(),
    ] {
        if logo != "https://example.test/" {
            cases.push(("logo_url", json!(logo)));
        }
    }
    cases.push((
        "logo_url",
        json!(format!("https://example.test/{}", "x".repeat(2048))),
    ));
    for currency in ["thb", "XYZ", ""] {
        cases.push(("currency_code", json!(currency)));
    }
    for methods in [
        json!([]),
        json!(["stripe", "stripe"]),
        json!(["cash"]),
        json!([null]),
    ] {
        cases.push(("payment_methods", methods));
    }
    cases.push(("preorder_enabled", json!(true)));
    cases.push(("unknown", json!(1)));
    for (field, value) in cases {
        let mut v = base.clone();
        v[field] = value;
        mutate(&c, &f, "POST", URI, v, 400).await;
    }
    let created = create(&c, &f, base).await;
    let path = format!("{URI}/{}", created["id"].as_str().unwrap());
    for patch in [
        json!({}),
        json!({"slug":"other"}),
        json!({"currency_code":"SGD"}),
        json!({"name":null}),
        json!({"preorder_enabled":null}),
        json!({"payment_methods":null}),
        json!({"event_starts_on":"2028-03-01"}),
        json!({"description_markdown":" "}),
        json!({"unknown":1}),
    ] {
        mutate(&c, &f, "PATCH", &path, patch, 400).await;
    }
    for query in [
        "page=0",
        "per_page=0",
        "per_page=101",
        "page=-1",
        "page=x",
        "page=1&page=2",
        "preorder_enabled=1",
        "preorder_enabled=null",
        "unknown=x",
    ] {
        let r = c
            .send(authed(
                "GET",
                &format!("{URI}?{query}"),
                f.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(r.status(), 400, "{query}");
    }
    for price in [
        "0",
        "0.00",
        "-1",
        "+1",
        "1e2",
        " 1",
        "1 ",
        ".1",
        "1.",
        "12.999",
        "NaN",
        "Infinity",
        "18446744073709551616",
    ] {
        mutate(
            &c,
            &f,
            "PUT",
            &format!("{path}/variants"),
            json!({"items":[item(2,price)]}),
            400,
        )
        .await;
    }
    for items in [
        json!([item(2, "12.99"), item(2, "1")]),
        json!(vec![item(2, "1"); 251]),
        json!([{"shopify_product_id":"gid://shopify/Product/1","shopify_variant_id":"gid://shopify/ProductVariant/2","final_price":12.99}]),
        json!([{"shopify_product_id":"gid://shopify/Location/1","shopify_variant_id":"gid://shopify/ProductVariant/2","final_price":"1"}]),
        json!([{"shopify_product_id":"gid://shopify/Product/1","shopify_variant_id":"gid://shopify/ProductVariant/0","final_price":"1"}]),
        json!([{"shopify_product_id":"gid://shopify/Product/1","shopify_variant_id":"gid://shopify/ProductVariant/18446744073709551616","final_price":"1"}]),
    ] {
        mutate(
            &c,
            &f,
            "PUT",
            &format!("{path}/variants"),
            json!({"items":items}),
            400,
        )
        .await;
    }
    for method in ["GET", "PATCH", "DELETE", "PUT"] {
        let uri = if method == "PUT" {
            format!("{URI}/invalid/variants")
        } else {
            format!("{URI}/invalid")
        };
        let r = c
            .send(authed_json(
                method,
                &uri,
                f.root_id(),
                AdminRole::Admin,
                &json!({"name":"ok","items":[]}),
            ))
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
    }
    assert_eq!(upstream.requests(), [] as [Value; 0]);
}
#[tokio::test]
async fn conventions_provider_failures_and_later_item_preserve_complete_selection() {
    let f = fixture().await;
    let c = client().await;
    let created = create(&c, &f, input(f.prefix())).await;
    let path = format!("{URI}/{}", created["id"].as_str().unwrap());
    let variants = format!("{path}/variants");
    mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"12.99")]}),
        503,
    )
    .await;
    for failure in [
        "http",
        "graphql",
        "partial",
        "malformed",
        "missing",
        "shape",
        "timeout",
    ] {
        let (c, upstream) = client_with_upstream_script(vec![remote(2)], failure).await;
        mutate(
            &c,
            &f,
            "PUT",
            &variants,
            json!({"items":[item(2,"12.99"),item(3,"20")]}),
            if failure == "timeout" { 504 } else { 502 },
        )
        .await;
        assert_eq!(upstream.requests().len(), 2);
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM convention_variants WHERE convention_id=$1")
                .bind(uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap())
                .fetch_one(&f.pool())
                .await
                .unwrap();
        assert_eq!(count, 0);
    }
    let (c, _upstream) = client_with_upstream_script(vec![remote(2)], "http").await;
    mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"12.99")]}),
        200,
    )
    .await;
    for failure in ["http", "graphql", "malformed", "timeout"] {
        let (c, _upstream) = client_with_upstream_script(vec![], failure).await;
        mutate(
            &c,
            &f,
            "PATCH",
            &path,
            json!({"preorder_enabled":true}),
            if failure == "timeout" { 504 } else { 502 },
        )
        .await;
        let r = c
            .send(authed("GET", &path, f.root_id(), AdminRole::Admin))
            .await
            .unwrap();
        assert_eq!(body_json(r).await["preorder_enabled"], false);
    }
}

#[tokio::test]
async fn conventions_two_currencies_share_shopify_identity_without_sharing_prices() {
    let f = fixture().await;
    let (c, upstream) = client_with_upstream_script(vec![remote(2), remote(2)], "http").await;
    let th = create(&c, &f, input(f.prefix())).await;
    let mut other = input(&format!("{}-sg", f.prefix()));
    other["currency_code"] = json!("SGD");
    let sg = create(&c, &f, other).await;
    for (convention, price) in [(&th, "12.99"), (&sg, "20.01")] {
        let path = format!("{URI}/{}/variants", convention["id"].as_str().unwrap());
        let saved = mutate(&c, &f, "PUT", &path, json!({"items":[item(2,price)]}), 200).await;
        assert_eq!(saved["items"][0]["final_price"], price);
    }
    assert_eq!(
        upstream.requests()[0]["variables"],
        json!({"id":"gid://shopify/ProductVariant/2"})
    );
    let rows:Vec<String>=sqlx::query_scalar("SELECT final_price::text FROM convention_variants WHERE convention_id=ANY($1) ORDER BY final_price").bind(vec![uuid::Uuid::parse_str(th["id"].as_str().unwrap()).unwrap(),uuid::Uuid::parse_str(sg["id"].as_str().unwrap()).unwrap()]).fetch_all(&f.pool()).await.unwrap();
    assert_eq!(rows, vec!["12.99", "20.01"]);
    let mut jp = input(&format!("{}-jp", f.prefix()));
    jp["currency_code"] = json!("JPY");
    let jp = create(&c, &f, jp).await;
    mutate(
        &c,
        &f,
        "PUT",
        &format!("{URI}/{}/variants", jp["id"].as_str().unwrap()),
        json!({"items":[item(2,"12.99")]}),
        400,
    )
    .await;
}
#[tokio::test]
async fn conventions_authentication_cookie_origins_and_put_preflight() {
    use axum::{body::Body, http::Request};
    let f = fixture().await;
    let c = client().await;
    let created = create(&c, &f, input(f.prefix())).await;
    let path = format!("{URI}/{}", created["id"].as_str().unwrap());
    let variants = format!("{path}/variants");
    let expired = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &json!({"sub":f.root_id(),"username":"fixture","role":"ADMIN","exp":1}),
        &jsonwebtoken::EncodingKey::from_secret(&hex::decode(crate::common::JWT_SECRET).unwrap()),
    )
    .unwrap();
    for (method, uri) in [
        ("GET", URI),
        ("POST", URI),
        ("GET", path.as_str()),
        ("PATCH", path.as_str()),
        ("DELETE", path.as_str()),
        ("GET", variants.as_str()),
        ("PUT", variants.as_str()),
    ] {
        for token in [None, Some("invalid"), Some(expired.as_str())] {
            let mut builder = Request::builder().method(method).uri(uri);
            if let Some(t) = token {
                builder = builder.header("authorization", format!("Bearer {t}"));
            }
            let r = c.send(builder.body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(r.status(), 401);
            assert_eq!(r.headers()["www-authenticate"], "Bearer");
        }
    }
    for role in [AdminRole::Admin, AdminRole::SuperAdmin] {
        let r = c
            .send(authed("GET", &path, f.root_id(), role))
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
    }
    let login = crate::v1::auth::cookie_login(&f.username("root"), "fixture-root-password").await;
    assert_eq!(login.status(), 200);
    let cookie = crate::v1::auth::cookie_pair(&login);
    for (method, uri, body) in [
        ("POST", URI, input(&format!("{}-cookie", f.prefix()))),
        ("PATCH", path.as_str(), json!({"name":"Cookie"})),
        ("PUT", variants.as_str(), json!({"items":[]})),
        ("DELETE", path.as_str(), json!({})),
    ] {
        for origin in [
            None,
            Some("https://evil.example"),
            Some("http://127.0.0.1:8084/"),
        ] {
            let mut builder = Request::builder()
                .method(method)
                .uri(uri)
                .header("x-session-mode", "cookie")
                .header("cookie", &cookie)
                .header("content-type", "application/json");
            if let Some(origin) = origin {
                builder = builder.header("origin", origin);
            }
            let r = c
                .send(
                    builder
                        .body(if method == "DELETE" {
                            Body::empty()
                        } else {
                            Body::from(body.to_string())
                        })
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(r.status(), 403);
        }
        let r = c
            .send(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("x-session-mode", "cookie")
                    .header("cookie", &cookie)
                    .header("origin", "http://127.0.0.1:8084")
                    .header("content-type", "application/json")
                    .body(if method == "DELETE" {
                        Body::empty()
                    } else {
                        Body::from(body.to_string())
                    })
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            r.status(),
            match method {
                "POST" => 201,
                "DELETE" => 204,
                _ => 200,
            }
        );
    }
    let r = c
        .send(
            Request::builder()
                .method("OPTIONS")
                .uri(&variants)
                .header("origin", "http://127.0.0.1:8084")
                .header("access-control-request-method", "PUT")
                .header(
                    "access-control-request-headers",
                    "content-type,x-session-mode",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert!(
        r.headers()["access-control-allow-methods"]
            .to_str()
            .unwrap()
            .split(',')
            .any(|m| m.trim() == "PUT")
    );
    assert_eq!(r.headers()["access-control-allow-credentials"], "true");
}
#[tokio::test]
async fn conventions_removed_capability_allows_read_disable_delete_but_not_enable_or_reprice() {
    let f = fixture().await;
    let (c, _upstream) = client_with_upstream_script(vec![remote(2), remote(2)], "http").await;
    let created = create(&c, &f, input(f.prefix())).await;
    let path = format!("{URI}/{}", created["id"].as_str().unwrap());
    let variants = format!("{path}/variants");
    mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"12.99")]}),
        200,
    )
    .await;
    mutate(
        &c,
        &f,
        "PATCH",
        &path,
        json!({"preorder_enabled":true}),
        200,
    )
    .await;
    let removed = crate::common::client_with_providers(crate::common::ProviderConfig {
        convention_currencies: Some("[]"),
        ..Default::default()
    })
    .await;
    let r = removed
        .send(authed("GET", &variants, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(body_json(r).await["items"][0]["final_price"], "12.99");
    mutate(
        &removed,
        &f,
        "PATCH",
        &path,
        json!({"name":"Metadata"}),
        200,
    )
    .await;
    mutate(
        &removed,
        &f,
        "PATCH",
        &path,
        json!({"preorder_enabled":false}),
        200,
    )
    .await;
    mutate(
        &removed,
        &f,
        "PATCH",
        &path,
        json!({"preorder_enabled":true}),
        400,
    )
    .await;
    mutate(
        &removed,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"20")]}),
        400,
    )
    .await;
    let r = removed
        .send(authed("DELETE", &path, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(r.status(), 204);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM convention_variants WHERE convention_id=$1")
            .bind(uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap())
            .fetch_one(&f.pool())
            .await
            .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn conventions_upstream_identity_policy_and_unavailable_failures() {
    let f = fixture().await;
    let c = client().await;
    let created = create(&c, &f, input(f.prefix())).await;
    let path = format!("{URI}/{}/variants", created["id"].as_str().unwrap());
    let invalid = crate::common::client_with_shopify("invalid-url").await;
    mutate(
        &invalid,
        &f,
        "PUT",
        &path,
        json!({"items":[item(2,"12.99")]}),
        503,
    )
    .await;
    let refused = crate::common::client_with_unreachable_shopify().await;
    mutate(
        &refused,
        &f,
        "PUT",
        &path,
        json!({"items":[item(2,"12.99")]}),
        502,
    )
    .await;
    let (wire, _wire_upstream) = crate::common::client_with_wire_script(vec![None]).await;
    mutate(
        &wire,
        &f,
        "PUT",
        &path,
        json!({"items":[item(2,"12.99")]}),
        502,
    )
    .await;
    for (key, value, status) in [
        ("id", json!("gid://shopify/ProductVariant/3"), 502),
        (
            "product",
            json!({"id":"gid://shopify/Product/2","status":"ACTIVE"}),
            400,
        ),
        (
            "product",
            json!({"id":"gid://shopify/Product/1","status":"DRAFT"}),
            400,
        ),
        ("inventoryPolicy", json!("CONTINUE"), 400),
        ("inventoryItem", json!({"tracked":false}), 400),
        ("inventoryPolicy", json!("UNKNOWN"), 502),
    ] {
        let mut data = remote(2);
        data["productVariant"][key] = value;
        let (c, _upstream) = client_with_upstream_script(vec![data], "http").await;
        mutate(
            &c,
            &f,
            "PUT",
            &path,
            json!({"items":[item(2,"12.99")]}),
            status,
        )
        .await;
    }
    for (data, status) in [
        (json!({}), 502),
        (json!({"productVariant":{}}), 502),
        (json!({"productVariant":null}), 400),
    ] {
        let (c, _upstream) = client_with_upstream_script(vec![data], "http").await;
        mutate(
            &c,
            &f,
            "PUT",
            &path,
            json!({"items":[item(2,"12.99")]}),
            status,
        )
        .await;
    }
}

#[tokio::test]
async fn conventions_database_constraints_and_later_write_or_commit_faults_roll_back() {
    let f = fixture().await;
    let (c, _upstream) = client_with_upstream_script(
        vec![
            remote(2),
            remote(3),
            remote(2),
            remote(4),
            remote(2),
            remote(4),
        ],
        "http",
    )
    .await;
    let created = create(&c, &f, input(f.prefix())).await;
    let id = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let path = format!("{URI}/{id}");
    let variants = format!("{path}/variants");
    let original = mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"12.99"),item(3,"30")]}),
        200,
    )
    .await;
    for value in ["0", "-1", "NaN", "Infinity", "-Infinity"] {
        assert!(sqlx::query("INSERT INTO convention_variants(convention_id,shopify_product_id,shopify_variant_id,final_price) VALUES($1,'gid://shopify/Product/1','gid://shopify/ProductVariant/99',$2::text::numeric)").bind(id).bind(value).execute(&f.pool()).await.is_err(),"{value}");
    }
    for deferred in [false, true] {
        let timing = if deferred { "CONSTRAINT" } else { "" };
        let position = if deferred { "AFTER" } else { "BEFORE" };
        let deferrable = if deferred {
            "DEFERRABLE INITIALLY DEFERRED"
        } else {
            ""
        };
        let prefix = f.prefix();
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {prefix}_variant_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'private convention storage failure'; END $$; CREATE {timing} TRIGGER {prefix}_variant_fault {position} INSERT OR UPDATE ON convention_variants {deferrable} FOR EACH ROW WHEN (NEW.convention_id='{id}'::uuid AND NEW.final_price=99.99) EXECUTE FUNCTION {prefix}_variant_fault();"))).execute(&f.pool()).await.unwrap();
        let body = mutate(
            &c,
            &f,
            "PUT",
            &variants,
            json!({"items":[item(2,"20"),item(4,"99.99")]}),
            500,
        )
        .await;
        assert_eq!(body, json!({"error":"Internal server error"}));
        let r = c
            .send(authed("GET", &variants, f.root_id(), AdminRole::Admin))
            .await
            .unwrap();
        assert_eq!(body_json(r).await["items"], original["items"]);
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP TRIGGER {prefix}_variant_fault ON convention_variants; DROP FUNCTION {prefix}_variant_fault();"))).execute(&f.pool()).await.unwrap();
    }
    for deferred in [false, true] {
        let prefix = f.prefix();
        let constraint = if deferred { "CONSTRAINT" } else { "" };
        let position = if deferred { "AFTER" } else { "BEFORE" };
        let deferrable = if deferred {
            "DEFERRABLE INITIALLY DEFERRED"
        } else {
            ""
        };
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE FUNCTION {prefix}_metadata_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'private metadata failure'; END $$; CREATE {constraint} TRIGGER {prefix}_metadata_fault {position} UPDATE OR DELETE ON conventions {deferrable} FOR EACH ROW WHEN (OLD.id='{id}'::uuid) EXECUTE FUNCTION {prefix}_metadata_fault();"))).execute(&f.pool()).await.unwrap();
        mutate(&c, &f, "PATCH", &path, json!({"name":"Failure"}), 500).await;
        let r = c
            .send(authed("DELETE", &path, f.root_id(), AdminRole::Admin))
            .await
            .unwrap();
        assert_eq!(r.status(), 500);
        let name: String = sqlx::query_scalar("SELECT name FROM conventions WHERE id=$1")
            .bind(id)
            .fetch_one(&f.pool())
            .await
            .unwrap();
        assert_eq!(name, "Convention");
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM convention_variants WHERE convention_id=$1")
                .bind(id)
                .fetch_one(&f.pool())
                .await
                .unwrap();
        assert_eq!(count, 2);
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!("DROP TRIGGER {prefix}_metadata_fault ON conventions; DROP FUNCTION {prefix}_metadata_fault();"))).execute(&f.pool()).await.unwrap();
    }
}
#[tokio::test]
async fn conventions_selection_change_during_enable_is_conflict_without_holding_database_lock() {
    let f = fixture().await;
    let (c, _upstream) = client_with_upstream_script(vec![remote(2)], "http").await;
    let created = create(&c, &f, input(f.prefix())).await;
    let id = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let path = format!("{URI}/{id}");
    let variants = format!("{path}/variants");
    mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"12.99")]}),
        200,
    )
    .await;
    let arrived = std::sync::Arc::new(tokio::sync::Notify::new());
    let release = std::sync::Arc::new(tokio::sync::Notify::new());
    let a = arrived.clone();
    let r = release.clone();
    let router = axum::Router::new().route(
        "/graphql.json",
        axum::routing::post(move || {
            let a = a.clone();
            let r = r.clone();
            async move {
                a.notify_one();
                r.notified().await;
                axum::Json(json!({"data":remote(2)}))
            }
        }),
    );
    let (blocking, _blocking_upstream) = crate::common::client_with_upstream(router).await;
    let cloned = blocking.clone();
    let root = f.root_id();
    let owned = path.clone();
    let enabling = tokio::spawn(async move {
        cloned
            .send(authed_json(
                "PATCH",
                &owned,
                root,
                AdminRole::Admin,
                &json!({"preorder_enabled":true}),
            ))
            .await
            .unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), arrived.notified())
        .await
        .unwrap();
    mutate(&c, &f, "PUT", &variants, json!({"items":[]}), 200).await;
    release.notify_one();
    let r = enabling.await.unwrap();
    assert_eq!(r.status(), 409);
    let r = c
        .send(authed("GET", &path, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(body_json(r).await["preorder_enabled"], false);
}

#[tokio::test]
async fn conventions_database_read_write_and_connection_errors_are_safe() {
    let f = fixture().await;
    let c = client().await;
    let created = create(&c, &f, input(f.prefix())).await;
    let path = format!("{URI}/{}", created["id"].as_str().unwrap());
    let variants = format!("{path}/variants");
    let role = format!("{}_database", f.prefix());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE ROLE {role} LOGIN PASSWORD 'e2e-convention'; GRANT USAGE ON SCHEMA public TO {role}; GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA public TO {role}; GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA public TO {role};"))).execute(&f.pool()).await.unwrap();
    let mut url = reqwest::Url::parse(&crate::common::database_url()).unwrap();
    url.set_username(&role).unwrap();
    url.set_password(Some("e2e-convention")).unwrap();
    let url = url.to_string();
    let restricted = crate::common::client_with_providers(crate::common::ProviderConfig {
        database_url: Some(&url),
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "REVOKE SELECT ON conventions FROM {role};"
    )))
    .execute(&f.pool())
    .await
    .unwrap();
    for uri in [URI, path.as_str(), variants.as_str()] {
        let r = restricted
            .send(authed("GET", uri, f.root_id(), AdminRole::Admin))
            .await
            .unwrap();
        assert_eq!(r.status(), 500);
        assert_eq!(body_json(r).await, json!({"error":"Internal server error"}));
    }
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("GRANT SELECT ON conventions TO {role}; REVOKE INSERT,UPDATE,DELETE ON conventions FROM {role};"))).execute(&f.pool()).await.unwrap();
    mutate(
        &restricted,
        &f,
        "POST",
        URI,
        input(&format!("{}-fault", f.prefix())),
        500,
    )
    .await;
    mutate(
        &restricted,
        &f,
        "PATCH",
        &path,
        json!({"name":"Denied"}),
        500,
    )
    .await;
    let r = restricted
        .send(authed("DELETE", &path, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(r.status(), 500);
    let name: String = sqlx::query_scalar("SELECT name FROM conventions WHERE id=$1")
        .bind(uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap())
        .fetch_one(&f.pool())
        .await
        .unwrap();
    assert_eq!(name, "Convention");
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("ALTER ROLE {role} NOLOGIN;")))
        .execute(&f.pool())
        .await
        .unwrap();
    sqlx::query("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE usename=$1 AND pid<>pg_backend_pid()").bind(&role).execute(&f.pool()).await.unwrap();
    let r = restricted
        .send(authed("GET", URI, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(r.status(), 500);
    assert_eq!(body_json(r).await, json!({"error":"Internal server error"}));
    drop(restricted);
}

#[tokio::test]
async fn conventions_accept_boundaries_paginate_and_allow_both_admin_roles() {
    let f = fixture().await;
    let replies = (1..=250).map(remote).collect();
    let (c, upstream) = client_with_upstream_script(replies, "http").await;
    for (index, role) in [AdminRole::Admin, AdminRole::SuperAdmin]
        .into_iter()
        .enumerate()
    {
        let mut value = input(&format!("{}-{index}", f.prefix()));
        value["name"] = json!("ก".repeat(120));
        value["description_markdown"] = json!("x".repeat(20000));
        value["logo_url"] = json!(format!("https://example.test/{}", "x".repeat(2027)));
        let r = c
            .send(authed_json("POST", URI, f.root_id(), role, &value))
            .await
            .unwrap();
        assert_eq!(r.status(), 201);
        let created = body_json(r).await;
        let path = format!("{URI}/{}", created["id"].as_str().unwrap());
        let variants = format!("{path}/variants");
        for uri in [URI, path.as_str(), variants.as_str()] {
            let r = c.send(authed("GET", uri, f.root_id(), role)).await.unwrap();
            assert_eq!(r.status(), 200);
        }
        for (method, uri, body) in [
            ("PATCH", path.as_str(), json!({"name":"Changed"})),
            ("PUT", variants.as_str(), json!({"items":[]})),
        ] {
            let r = c
                .send(authed_json(method, uri, f.root_id(), role, &body))
                .await
                .unwrap();
            assert_eq!(r.status(), 200);
        }
        let r = c
            .send(authed("DELETE", &path, f.root_id(), role))
            .await
            .unwrap();
        assert_eq!(r.status(), 204);
    }
    let created = create(&c, &f, input(f.prefix())).await;
    let path = format!("{URI}/{}", created["id"].as_str().unwrap());
    let variants = format!("{path}/variants");
    let duplicate = c
        .send(authed_json(
            "POST",
            URI,
            f.root_id(),
            AdminRole::Admin,
            &input(f.prefix()),
        ))
        .await
        .unwrap();
    assert_eq!(duplicate.status(), 409);
    let saved = mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":(1..=250).map(|i|item(i,"12.99")).collect::<Vec<_>>()}),
        200,
    )
    .await;
    assert_eq!(saved["items"].as_array().unwrap().len(), 250);
    let mut ids = std::collections::HashSet::new();
    for page in [1, 2, 3] {
        let r = c
            .send(authed(
                "GET",
                &format!("{variants}?page={page}&per_page=100"),
                f.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let body = body_json(r).await;
        assert_eq!(body["has_next"], page < 3);
        for row in body["items"].as_array().unwrap() {
            assert!(ids.insert(row["id"].as_str().unwrap().to_owned()));
        }
    }
    assert_eq!(ids.len(), 250);
    assert_eq!(upstream.requests().len(), 250);
    let missing = format!("{URI}/{}", uuid::Uuid::new_v4());
    for (method, uri, body) in [
        ("GET", missing.clone(), json!({})),
        ("PATCH", missing.clone(), json!({"name":"Changed"})),
        ("DELETE", missing.clone(), json!({})),
        ("GET", format!("{missing}/variants"), json!({})),
        ("PUT", format!("{missing}/variants"), json!({"items":[]})),
    ] {
        let request = if method == "DELETE" {
            authed(method, &uri, f.root_id(), AdminRole::Admin)
        } else {
            authed_json(method, &uri, f.root_id(), AdminRole::Admin, &body)
        };
        let r = c.send(request).await.unwrap();
        assert_eq!(r.status(), 404);
    }
}
#[tokio::test]
async fn conventions_wrong_types_nulls_and_unknown_fields_never_reach_shopify() {
    let f = fixture().await;
    let (c, upstream) = client_with_upstream_script(vec![], "http").await;
    for field in [
        "slug",
        "name",
        "logo_url",
        "country_code",
        "event_starts_on",
        "event_ends_on",
        "description_markdown",
        "currency_code",
        "payment_methods",
    ] {
        for value in [json!(1), json!(true), json!({})] {
            let mut v = input(f.prefix());
            v[field] = value;
            mutate(&c, &f, "POST", URI, v, 400).await;
        }
    }
    let created = create(&c, &f, input(f.prefix())).await;
    let path = format!("{URI}/{}", created["id"].as_str().unwrap());
    let variants = format!("{path}/variants");
    for field in [
        "name",
        "country_code",
        "event_starts_on",
        "event_ends_on",
        "description_markdown",
        "payment_methods",
        "preorder_enabled",
    ] {
        for value in [Value::Null, json!(1), json!({})] {
            let mut v = json!({});
            v[field] = value;
            mutate(&c, &f, "PATCH", &path, v, 400).await;
        }
    }
    for field in ["shopify_product_id", "shopify_variant_id", "final_price"] {
        for value in [Value::Null, json!(1), json!(true), json!({})] {
            let mut v = item(2, "12.99");
            v[field] = value;
            mutate(&c, &f, "PUT", &variants, json!({"items":[v]}), 400).await;
        }
        let mut v = item(2, "12.99");
        v.as_object_mut().unwrap().remove(field);
        mutate(&c, &f, "PUT", &variants, json!({"items":[v]}), 400).await;
    }
    for value in [
        json!({}),
        json!({"items":null}),
        json!({"items":{}}),
        json!({"items":[],"unknown":true}),
        json!({"items":[{"unknown":1}]}),
    ] {
        mutate(&c, &f, "PUT", &variants, value, 400).await;
    }
    for query in [
        "page=0",
        "per_page=101",
        "preorder_enabled=true",
        "unknown=x",
        "page=1&page=2",
    ] {
        let r = c
            .send(authed(
                "GET",
                &format!("{variants}?{query}"),
                f.root_id(),
                AdminRole::Admin,
            ))
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
    }
    assert_eq!(upstream.requests(), [] as [Value; 0]);
}
#[tokio::test]
async fn conventions_method_changes_recheck_existing_prices_before_enabling() {
    let f = fixture().await;
    let (url, upstream) =
        crate::common::scripted_upstream(vec![remote(2), remote(2), remote(2)], "http").await;
    let c = crate::common::client_with_providers(crate::common::ProviderConfig {
        shopify_url: Some(&url),
        convention_currencies: Some(r#"[{code="HUF",payment_methods=["stripe","paypal"]}]"#),
        ..Default::default()
    })
    .await;
    let mut value = input(f.prefix());
    value["currency_code"] = json!("HUF");
    value["payment_methods"] = json!(["stripe"]);
    let created = create(&c, &f, value).await;
    let path = format!("{URI}/{}", created["id"].as_str().unwrap());
    let variants = format!("{path}/variants");
    mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"12.99")]}),
        200,
    )
    .await;
    mutate(
        &c,
        &f,
        "PATCH",
        &path,
        json!({"payment_methods":["paypal"]}),
        400,
    )
    .await;
    mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"12")]}),
        200,
    )
    .await;
    mutate(
        &c,
        &f,
        "PATCH",
        &path,
        json!({"payment_methods":["paypal"]}),
        200,
    )
    .await;
    mutate(
        &c,
        &f,
        "PATCH",
        &path,
        json!({"preorder_enabled":true}),
        200,
    )
    .await;
    mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"12.99")]}),
        400,
    )
    .await;
    let r = c
        .send(authed("GET", &path, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(body_json(r).await["payment_methods"], json!(["paypal"]));
    assert_eq!(upstream.requests().len(), 3);
}
#[tokio::test]
async fn conventions_concurrent_disjoint_metadata_patches_preserve_omitted_fields() {
    let f = fixture().await;
    let c = client().await;
    let other = crate::common::client_with_providers(crate::common::ProviderConfig {
        scheduler_enabled: Some(false),
        ..Default::default()
    })
    .await;
    let created = create(&c, &f, input(f.prefix())).await;
    let id = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let path = format!("{URI}/{id}");
    let pool = f.pool();
    let mut control = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM conventions WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_one(&mut *control)
        .await
        .unwrap();
    let root = f.root_id();
    let name_path = path.clone();
    let description_path = path.clone();
    let first = c.clone();
    let name = tokio::spawn(async move {
        first
            .send(authed_json(
                "PATCH",
                &name_path,
                root,
                AdminRole::Admin,
                &json!({"name":"Name"}),
            ))
            .await
            .unwrap()
    });
    let description = tokio::spawn(async move {
        other
            .send(authed_json(
                "PATCH",
                &description_path,
                root,
                AdminRole::Admin,
                &json!({"description_markdown":"Description"}),
            ))
            .await
            .unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(5),async {loop {let count:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE 'SELECT * FROM conventions WHERE id=$1 FOR UPDATE%'").fetch_one(&pool).await.unwrap();if count>=2 {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}}).await.unwrap();
    control.commit().await.unwrap();
    assert_eq!(name.await.unwrap().status(), 200);
    assert_eq!(description.await.unwrap().status(), 200);
    let r = c
        .send(authed("GET", &path, root, AdminRole::Admin))
        .await
        .unwrap();
    let saved = body_json(r).await;
    assert_eq!(saved["name"], "Name");
    assert_eq!(saved["description_markdown"], "Description");
}

#[tokio::test]
async fn conventions_concurrent_variant_saves_use_last_completed_save_without_partial_state() {
    let f = fixture().await;
    let (c, _upstream) = client_with_upstream_script(vec![remote(2), remote(2)], "http").await;
    let created = create(&c, &f, input(f.prefix())).await;
    let variants = format!("{URI}/{}/variants", created["id"].as_str().unwrap());
    let original = mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"12.99")]}),
        200,
    )
    .await;
    let arrived = std::sync::Arc::new(tokio::sync::Notify::new());
    let release = std::sync::Arc::new(tokio::sync::Notify::new());
    let a = arrived.clone();
    let r = release.clone();
    let router = axum::Router::new().route(
        "/graphql.json",
        axum::routing::post(move || {
            let a = a.clone();
            let r = r.clone();
            async move {
                a.notify_one();
                r.notified().await;
                axum::Json(json!({"data":remote(2)}))
            }
        }),
    );
    let (blocking, _blocked_upstream) = crate::common::client_with_upstream(router).await;
    let root = f.root_id();
    let uri = variants.clone();
    let late = tokio::spawn(async move {
        blocking
            .send(authed_json(
                "PUT",
                &uri,
                root,
                AdminRole::Admin,
                &json!({"items":[item(2,"20")]}),
            ))
            .await
            .unwrap()
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), arrived.notified())
        .await
        .unwrap();
    let earlier = mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"30")]}),
        200,
    )
    .await;
    assert_eq!(earlier["items"][0]["final_price"], "30.00");
    release.notify_one();
    let response = late.await.unwrap();
    assert_eq!(response.status(), 200);
    let saved = body_json(response).await;
    assert_eq!(saved["items"][0]["final_price"], "20.00");
    assert_eq!(saved["items"][0]["id"], original["items"][0]["id"]);
    let r = c
        .send(authed("GET", &variants, root, AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(body_json(r).await["items"], saved["items"]);
}

#[tokio::test]
async fn conventions_enable_dependency_availability_and_later_provider_failure_keep_saved_prices() {
    let f = fixture().await;
    let (c, _upstream) = client_with_upstream_script(vec![remote(2)], "http").await;
    let created = create(&c, &f, input(f.prefix())).await;
    let path = format!("{URI}/{}", created["id"].as_str().unwrap());
    let variants = format!("{path}/variants");
    let original = mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"12.99")]}),
        200,
    )
    .await;
    let unavailable = client().await;
    let invalid = crate::common::client_with_shopify("invalid-url").await;
    let refused = crate::common::client_with_unreachable_shopify().await;
    let (wire, _wire_upstream) = crate::common::client_with_wire_script(vec![None]).await;
    for (c, status) in [
        (unavailable, 503),
        (invalid, 503),
        (refused, 502),
        (wire, 502),
    ] {
        mutate(
            &c,
            &f,
            "PATCH",
            &path,
            json!({"preorder_enabled":true}),
            status,
        )
        .await;
    }
    let (failing, upstream) = client_with_upstream_script(vec![remote(2)], "graphql").await;
    mutate(
        &failing,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"20"),item(3,"30")]}),
        502,
    )
    .await;
    assert_eq!(upstream.requests().len(), 2);
    let r = c
        .send(authed("GET", &variants, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(body_json(r).await["items"], original["items"]);
    let r = c
        .send(authed("GET", &path, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(body_json(r).await["preorder_enabled"], false);
}

#[tokio::test]
async fn conventions_reject_extra_query_and_delete_payload_without_effects() {
    use axum::{body::Body, http::Request};
    let f = fixture().await;
    let (c, upstream) = client_with_upstream_script(vec![remote(2)], "http").await;
    let created = create(&c, &f, input(f.prefix())).await;
    let id = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let path = format!("{URI}/{id}");
    let variants = format!("{path}/variants");
    let original = mutate(
        &c,
        &f,
        "PUT",
        &variants,
        json!({"items":[item(2,"12.99")]}),
        200,
    )
    .await;
    for (method, uri, body) in [
        (
            "POST",
            format!("{URI}?unknown=value"),
            input(&format!("{}-extra", f.prefix())),
        ),
        ("GET", format!("{path}?unknown=value"), json!({})),
        (
            "PATCH",
            format!("{path}?unknown=value"),
            json!({"name":"Must not change","preorder_enabled":true}),
        ),
        ("DELETE", format!("{path}?unknown=value"), json!({})),
        (
            "PUT",
            format!("{variants}?unknown=value"),
            json!({"items":[item(3,"20")]}),
        ),
    ] {
        let request = if method == "GET" || method == "DELETE" {
            authed(method, &uri, f.root_id(), AdminRole::Admin)
        } else {
            authed_json(method, &uri, f.root_id(), AdminRole::Admin, &body)
        };
        let response = c.send(request).await.unwrap();
        assert_eq!(response.status(), 400, "{method} {uri}");
    }
    for bytes in [
        b"{}".to_vec(),
        b"{\"unknown\":1}".to_vec(),
        b"null".to_vec(),
        b" ".to_vec(),
        vec![255],
    ] {
        let response=c.send(Request::builder().method("DELETE").uri(&path).header("authorization",authed("DELETE",&path,f.root_id(),AdminRole::Admin).headers()["authorization"].clone()).body(Body::from(bytes)).unwrap()).await.unwrap();
        assert_eq!(response.status(), 400);
    }
    let (name,enabled,count):(String,bool,i64)=sqlx::query_as("SELECT name,preorder_enabled,(SELECT count(*) FROM convention_variants WHERE convention_id=$1) FROM conventions WHERE id=$1").bind(id).fetch_one(&f.pool()).await.unwrap();
    assert_eq!(name, "Convention");
    assert!(!enabled);
    assert_eq!(count, 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM conventions WHERE left(slug,12)=$1")
        .bind(f.prefix())
        .fetch_one(&f.pool())
        .await
        .unwrap();
    assert_eq!(count, 1);
    let response = c
        .send(authed("GET", &variants, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(body_json(response).await["items"], original["items"]);
    assert_eq!(upstream.requests().len(), 1);
    let response = c
        .send(authed("DELETE", &path, f.root_id(), AdminRole::Admin))
        .await
        .unwrap();
    assert_eq!(response.status(), 204);
}

use super::*;

// Scenarios: empty/full pagination, ongoing/date boundary, country precedence/fallback,
// stable ties, disabled/expired discovery, enabled-only direct links, safe projection,
// strict queries/slugs/bodies and anonymous CORS through the production router.
#[tokio::test]
async fn directory_orders_country_then_dates_and_includes_ongoing_only() {
    let f = fixture().await;
    let c = common::client().await;
    let empty = get(&c, &format!("{URI}?today=2099-12-31")).await;
    assert_eq!(empty["items"], json!([]));
    assert_eq!(empty["has_next"], false);
    let (_, sg) = seed(&f, "sg", "SG", "2090-01-03", "2090-01-05", true, "SGD").await;
    let (_, th_b) = seed(&f, "th-b", "TH", "2090-01-04", "2090-01-05", true, "THB").await;
    let (_, th_a) = seed(&f, "th-a", "TH", "2090-01-04", "2090-01-05", true, "THB").await;
    let (_, ongoing) = seed(&f, "ongoing", "TH", "2089-12-31", "2090-01-02", true, "THB").await;
    seed(&f, "past", "TH", "2089-12-30", "2090-01-01", true, "THB").await;
    seed(
        &f,
        "disabled",
        "TH",
        "2090-01-02",
        "2090-01-04",
        false,
        "THB",
    )
    .await;
    let path = format!("{URI}?today=2090-01-02&locale_country=TH&per_page=2");
    let first_page = get(&c, &path).await;
    assert_eq!(first_page["visitor_country"], "TH");
    assert_eq!(first_page["items"][0]["slug"], ongoing);
    assert_eq!(first_page["items"][1]["slug"], th_a);
    assert_eq!(first_page["has_next"], true);
    let second_page = get(&c, &format!("{path}&page=2")).await;
    assert_eq!(second_page["items"][0]["slug"], th_b);
    assert_eq!(second_page["items"][1]["slug"], sg);
    assert_eq!(second_page["has_next"], false);
    for (header, country, expected) in [
        ("SG", Some("SG"), sg.as_str()),
        ("XX", Some("TH"), ongoing.as_str()),
        ("unknown", Some("TH"), ongoing.as_str()),
    ] {
        let mut req = request("GET", &path, None);
        req.headers_mut()
            .insert("x-vercel-ip-country", header.parse().unwrap());
        let r = c.send(req).await.unwrap();
        assert_eq!(r.status(), 200);
        let body = body_json(r).await;
        assert_eq!(body["visitor_country"], json!(country));
        assert_eq!(body["items"][0]["slug"], expected);
    }
    let unknown = get(&c, &format!("{URI}?today=2090-01-02")).await;
    assert_eq!(unknown["visitor_country"], Value::Null);
    assert_eq!(unknown["items"][0]["slug"], ongoing);
    let today = get(&c, &format!("{URI}?today=2090-01-05")).await;
    assert_eq!(today["items"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn detail_uses_enabled_authority_and_customer_only_projection() {
    let f = fixture().await;
    let c = common::client().await;
    let (_, slug) = event(&f).await;
    let (_, disabled) = seed(
        &f,
        "disabled",
        "TH",
        "2020-01-01",
        "2020-01-01",
        false,
        "THB",
    )
    .await;
    let body = get(&c, &format!("{URI}/{slug}")).await;
    assert_eq!(body["name"], "Customer event");
    assert_eq!(body["logo_preview_url"], Value::Null);
    assert_eq!(
        body["description_markdown"],
        "# Welcome\n<script>private HTML</script>"
    );
    for key in [
        "logo_url",
        "preorder_enabled",
        "payment_methods",
        "created_at",
        "updated_at",
    ] {
        assert!(body.get(key).is_none(), "{key}");
    }
    for base in [URI, "/v1/conventions"] {
        assert_eq!(get(&c, &format!("{base}/{slug}")).await["slug"], slug);
        get(&c, &format!("{base}?today=2028-01-01")).await;
    }
    for slug in [disabled.as_str(), "missing-event"] {
        for suffix in ["", "/products", "/quote"] {
            let (method, body) = if suffix == "/quote" {
                ("POST", Some(quote(vec![])))
            } else {
                ("GET", None)
            };
            assert_eq!(
                response(&c, method, &format!("{URI}/{slug}{suffix}"), body, 404).await,
                json!({"error":"Convention unavailable"})
            );
        }
    }
    let mut req = request("GET", &format!("{URI}/{slug}"), None);
    req.headers_mut()
        .insert("origin", "https://convention.enzumeii.com".parse().unwrap());
    let r = c.send(req).await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.headers()["access-control-allow-origin"],
        "https://convention.enzumeii.com"
    );
}

#[tokio::test]
async fn public_gets_reject_every_query_path_and_body_validation_class() {
    let f = fixture().await;
    let c = common::client().await;
    let (_, slug) = event(&f).await;
    for query in [
        "",
        "today=2026-2-01",
        "today=2027-02-29",
        "today=2026-01-01&extra=x",
        "today=2026-01-01&page=0",
        "today=2026-01-01&page=no",
        "today=2026-01-01&per_page=0",
        "today=2026-01-01&per_page=101",
        "today=2026-01-01&per_page=no",
        "today=2026-01-01&locale_country=XX",
        "today=2026-01-01&locale_country=USA",
    ] {
        response(&c, "GET", &format!("{URI}?{query}"), None, 400).await;
    }
    for slug in [
        "API",
        "api",
        "assets",
        "has--gap",
        "-start",
        "end-",
        "UPPER",
        "abc_123",
        &"a".repeat(64),
    ] {
        for suffix in ["", "/products", "/quote"] {
            let (method, body) = if suffix == "/quote" {
                ("POST", Some(quote(vec![])))
            } else {
                ("GET", None)
            };
            response(&c, method, &format!("{URI}/{slug}{suffix}"), body, 400).await;
        }
    }
    for path in [
        format!("{URI}?today=2028-01-01"),
        format!("{URI}/{slug}"),
        format!("{URI}/{slug}/products"),
    ] {
        response(&c, "GET", &path, Some(json!({})), 400).await;
    }
    for suffix in ["", "/products", "/quote"] {
        let (method, body) = if suffix == "/quote" {
            ("POST", Some(quote(vec![])))
        } else {
            ("GET", None)
        };
        response(
            &c,
            method,
            &format!("{URI}/{slug}{suffix}?extra=x"),
            body,
            400,
        )
        .await;
    }
}

#[tokio::test]
async fn logos_reuse_signed_projection_and_preserve_metadata_on_signing_failure() {
    let f = fixture().await;
    let (id, slug) = event(&f).await;
    let location = format!("conventions/{slug}/{}.png", Uuid::new_v4());
    sqlx::query(
        "UPDATE conventions
         SET logo_url = $2
         WHERE id = $1",
    )
    .bind(id)
    .bind(&location)
    .execute(&f.pool())
    .await
    .unwrap();
    let c = common::client().await;
    let detail = get(&c, &format!("{URI}/{slug}")).await;
    let preview = detail["logo_preview_url"].as_str().unwrap();
    assert!(preview.contains("X-Amz-Signature="));
    assert!(detail["logo_preview_url_expires_at"].as_str().is_some());
    let directory = get(&c, &format!("{URI}?today=2028-01-01")).await;
    assert!(
        directory["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["slug"] == slug && item["logo_preview_url"].as_str().is_some())
    );
    sqlx::query(
        "UPDATE conventions
         SET logo_url = 'invalid'
         WHERE id = $1",
    )
    .bind(id)
    .execute(&f.pool())
    .await
    .unwrap();
    let detail = get(&c, &format!("{URI}/{slug}")).await;
    assert_eq!(detail["logo_preview_url"], Value::Null);
    assert_eq!(detail["name"], "Customer event");
}

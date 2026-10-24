use super::*;

// Scenarios: empty/exact zero/two/three decimal money, cart line and total bounds,
// all request shapes/content types, duplicate/nonmember pre-provider rejection,
// unavailable variants/stock conflict, and no provider mutation or stored writes.
#[tokio::test]
async fn quotes_use_exact_saved_prices_and_allow_empty_cart_without_provider() {
    let f = fixture().await;
    let c = common::client().await;
    for (code, price, quantity, total) in [
        ("JPY", "12", 3, "36"),
        ("THB", "12.99", 3, "38.97"),
        ("KWD", "1.234", 3, "3.702"),
    ] {
        let (id, slug) = seed(
            &f,
            &code.to_ascii_lowercase(),
            "TH",
            "2020-01-01",
            "2020-01-02",
            true,
            code,
        )
        .await;
        let empty = response(
            &c,
            "POST",
            &format!("{URI}/{slug}/quote"),
            Some(quote(vec![])),
            200,
        )
        .await;
        let zero = if code == "JPY" {
            "0"
        } else if code == "KWD" {
            "0.000"
        } else {
            "0.00"
        };
        assert_eq!(
            empty,
            json!({"currency_code":code,"items":[],"subtotal":zero,"shipping":zero,"total":zero})
        );
        select(&f, id, 2, price).await;
        let (client, upstream) = common::client_with_upstream_script(
            vec![nodes(vec![live(2, quantity, "DENY")])],
            "http",
        )
        .await;
        let body = response(
            &client,
            "POST",
            &format!("{URI}/{slug}/quote"),
            Some(quote(vec![line(2, u32::try_from(quantity).unwrap())])),
            200,
        )
        .await;
        assert_eq!(body["total"], total);
        assert_eq!(body["shipping"], zero);
        assert_eq!(body["items"][0]["unit_price"], price);
        assert_eq!(body["items"][0]["line_total"], total);
        assert_eq!(upstream.requests().len(), 1);
        assert_eq!(upstream.requests()[0]["operationName"], "VariantNodes");
        let stored: String = sqlx::query_scalar(
            "SELECT final_price::text
             FROM convention_variants
             WHERE convention_id = $1",
        )
        .bind(id)
        .fetch_one(&f.pool())
        .await
        .unwrap();
        assert_eq!(stored, price);
    }
}

#[tokio::test]
async fn quotes_sum_multiple_lines_and_use_latest_price_not_client_fields() {
    let f = fixture().await;
    let (id, slug) = event(&f).await;
    select(&f, id, 2, "12.99").await;
    select(&f, id, 3, "20.01").await;
    sqlx::query(
        "UPDATE convention_variants
         SET final_price = 15.25
         WHERE convention_id = $1
           AND shopify_variant_id = $2",
    )
    .bind(id)
    .bind(gid(2))
    .execute(&f.pool())
    .await
    .unwrap();
    let (c, _upstream) = common::client_with_upstream_script(
        vec![nodes(vec![live(2, 4, "DENY"), live(3, -10, "CONTINUE")])],
        "http",
    )
    .await;
    let body = response(
        &c,
        "POST",
        &format!("{URI}/{slug}/quote"),
        Some(quote(vec![line(2, 2), line(3, 3)])),
        200,
    )
    .await;
    assert_eq!(body["total"], "90.53");
    assert_eq!(body["subtotal"], "90.53");
    assert_eq!(body["shipping"], "0.00");
}

#[tokio::test]
async fn every_quote_input_rejection_precedes_shopify_access() {
    let f = fixture().await;
    let (id, slug) = event(&f).await;
    select(&f, id, 2, "12.99").await;
    let (other, _) = seed(&f, "other", "SG", "2028-01-01", "2028-01-02", true, "SGD").await;
    select(&f, other, 3, "20").await;
    let (c, upstream) = common::client_with_upstream_script(vec![], "http").await;
    let path = format!("{URI}/{slug}/quote");
    let mut invalid = vec![
        json!({}),
        json!(null),
        json!([]),
        json!({"items":null}),
        json!({"items":{}}),
        json!({"items":[],"total":"1"}),
        quote(vec![json!({})]),
        quote(vec![json!({"variant_id":gid(2),"quantity":null})]),
        quote(vec![json!({"variant_id":gid(2),"quantity":1.2})]),
        quote(vec![json!({"variant_id":gid(2),"quantity":"1"})]),
        quote(vec![json!({"variant_id":gid(2),"quantity":-1})]),
        quote(vec![
            json!({"variant_id":gid(2),"quantity":4_294_967_296_u64}),
        ]),
        quote(vec![line(2, 0)]),
        quote(vec![line(2, 1000)]),
        quote(vec![
            json!({"variant_id":gid(2),"quantity":1,"unit_price":"0.01"}),
        ]),
        quote(vec![line(2, 1), line(2, 2)]),
        quote(vec![line(3, 1)]),
    ];
    invalid.extend([
        quote(vec![json!({"quantity":1})]),
        quote(vec![json!({"variant_id":gid(2)})]),
        quote(vec![json!({"variant_id":null,"quantity":1})]),
        quote(vec![json!({"variant_id":2,"quantity":1})]),
        quote(vec![json!({"variant_id":gid(2),"quantity":true})]),
        quote(vec![json!(2)]),
    ]);
    for value in [
        "bad",
        "gid://shopify/Product/2",
        "gid://shopify/ProductVariant/0",
        "gid://shopify/ProductVariant/-2",
    ] {
        invalid.push(quote(vec![json!({"variant_id":value,"quantity":1})]));
    }
    invalid.push(quote((1..=251).map(|n| line(n, 1)).collect()));
    for body in invalid {
        response(&c, "POST", &path, Some(body), 400).await;
    }
    for (content, body) in [
        (Some("application/json"), "{"),
        (Some("text/plain"), r#"{"items":[]}"#),
        (None, r#"{"items":[]}"#),
    ] {
        let mut builder = Request::builder().method("POST").uri(&path);
        if let Some(content) = content {
            builder = builder.header("content-type", content);
        }
        let r = c
            .send(builder.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
    }
    assert_eq!(upstream.requests().len(), 0);
}

#[tokio::test]
async fn selected_unavailable_or_overstock_lines_never_return_partial_totals() {
    let f = fixture().await;
    let (id, slug) = event(&f).await;
    select(&f, id, 2, "12.99").await;
    select(&f, id, 3, "20").await;
    let mut untracked = live(3, 5, "DENY");
    untracked["inventoryItem"]["tracked"] = json!(false);
    untracked["inventoryQuantity"] = Value::Null;
    let mut draft = live(3, 5, "DENY");
    draft["product"]["status"] = json!("ARCHIVED");
    draft["inventoryQuantity"] = Value::Null;
    let mut changed = live(3, 5, "DENY");
    changed["product"]["id"] = json!("gid://shopify/Product/9");
    let replies = [
        Value::Null,
        untracked,
        draft,
        changed,
        live(3, 0, "DENY"),
        live(3, -1, "DENY"),
        live(3, 1, "DENY"),
    ]
    .into_iter()
    .map(|bad| nodes(vec![live(2, 5, "DENY"), bad]))
    .collect();
    let (c, upstream) = common::client_with_upstream_script(replies, "http").await;
    for _ in 0..7 {
        let body = response(
            &c,
            "POST",
            &format!("{URI}/{slug}/quote"),
            Some(quote(vec![line(2, 1), line(3, 2)])),
            409,
        )
        .await;
        assert!(body.get("total").is_none());
        assert!(body.get("items").is_none());
    }
    assert_eq!(upstream.requests().len(), 7);
}

#[tokio::test]
async fn maximum_lines_quantities_and_checked_arithmetic() {
    let f = fixture().await;
    let (id, slug) = event(&f).await;
    for n in 1..=250 {
        select(&f, id, n, "0.01").await;
    }
    let wire = nodes((1..=250).map(|n| live(n, 0, "CONTINUE")).collect());
    let (c, upstream) = common::client_with_upstream_script(vec![wire.clone(), wire], "http").await;
    let catalogue = get(&c, &format!("{URI}/{slug}/products")).await;
    assert_eq!(
        catalogue["products"][0]["variants"]
            .as_array()
            .unwrap()
            .len(),
        250
    );
    let body = response(
        &c,
        "POST",
        &format!("{URI}/{slug}/quote"),
        Some(quote((1..=250).map(|n| line(n, 999)).collect())),
        200,
    )
    .await;
    assert_eq!(body["total"], "2497.50");
    assert_eq!(body["items"].as_array().unwrap().len(), 250);
    assert_eq!(
        upstream.requests()[0]["variables"]["ids"]
            .as_array()
            .unwrap()
            .len(),
        250
    );
    sqlx::query(
        "UPDATE convention_variants
         SET final_price = 92233720368547758.07
         WHERE convention_id = $1",
    )
    .bind(id)
    .execute(&f.pool())
    .await
    .unwrap();
    let (c, _upstream) = common::client_with_upstream_script(
        vec![
            nodes(vec![live(1, 3, "DENY")]),
            nodes(vec![live(1, 3, "DENY"), live(2, 3, "DENY")]),
        ],
        "http",
    )
    .await;
    response(
        &c,
        "POST",
        &format!("{URI}/{slug}/quote"),
        Some(quote(vec![line(1, 2)])),
        400,
    )
    .await;
    response(
        &c,
        "POST",
        &format!("{URI}/{slug}/quote"),
        Some(quote(vec![line(1, 1), line(2, 1)])),
        400,
    )
    .await;
}

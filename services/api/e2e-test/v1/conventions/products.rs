use super::*;

// Scenarios: one bounded nodes read, saved-only selection, full grouping, current
// public metadata, trusted persisted prices, tracked ACTIVE eligibility, missing
// nodes, DENY stock cap and CONTINUE zero/negative stock, and malformed wire data.
#[tokio::test]
async fn products_are_selected_current_grouped_and_use_only_saved_prices() {
    let f = fixture().await;
    let (id, slug) = event(&f).await;
    for (number, price) in [
        (2, "12.99"),
        (3, "20"),
        (4, "1"),
        (5, "2"),
        (6, "3"),
        (7, "4"),
        (8, "5"),
    ] {
        select(&f, id, number, price).await;
    }
    let mut draft = live(6, 3, "DENY");
    draft["product"]["status"] = json!("DRAFT");
    draft["inventoryQuantity"] = Value::Null;
    let mut untracked = live(7, 3, "DENY");
    untracked["inventoryItem"]["tracked"] = json!(false);
    untracked["inventoryQuantity"] = Value::Null;
    let wire = nodes(vec![
        live(2, 4, "DENY"),
        live(3, 0, "DENY"),
        live(4, -2, "CONTINUE"),
        Value::Null,
        draft,
        untracked,
        live(8, 3000, "DENY"),
    ]);
    let (c, upstream) = common::client_with_upstream_script(vec![wire], "http").await;
    let body = get(&c, &format!("{URI}/{slug}/products")).await;
    assert_eq!(body["convention"]["currency_code"], "THB");
    assert_eq!(body["payment_methods"], json!(["stripe"]));
    assert_eq!(body["products"].as_array().unwrap().len(), 1);
    let product = &body["products"][0];
    assert_eq!(product["title"], "Fresh title");
    assert_eq!(product["description"], "Fresh description");
    assert_eq!(product["product_type"], "Print");
    assert_eq!(product["variants"].as_array().unwrap().len(), 4);
    let variants = &product["variants"];
    assert_eq!(variants[0]["id"], gid(2));
    assert_eq!(variants[0]["final_price"], "12.99");
    assert_eq!(variants[0]["max_quantity"], 4);
    assert_eq!(
        variants[0]["selected_options"],
        json!([{"name":"Color","value":"Blue"}])
    );
    assert_eq!(variants[1]["max_quantity"], 0);
    assert_eq!(variants[2]["available_quantity"], -2);
    assert_eq!(variants[2]["max_quantity"], 999);
    assert_eq!(variants[3]["max_quantity"], 999);
    let text = body.to_string();
    for key in [
        "contextualPricing",
        "unit_cost",
        "payment_methods",
        "created_at",
        "logo_url",
    ] {
        assert!(!text.contains(key), "{key}");
    }
    let requests = upstream.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["operationName"], "VariantNodes");
    assert_eq!(
        requests[0]["variables"]["ids"],
        json!((2..=8).map(gid).collect::<Vec<_>>())
    );
    assert!(!requests[0]["query"].as_str().unwrap().contains("mutation"));
}

#[tokio::test]
async fn empty_selected_catalogue_does_not_require_shopify() {
    let f = fixture().await;
    let (_, slug) = event(&f).await;
    let c = common::client().await;
    let result = get(&c, &format!("{URI}/{slug}/products")).await;
    assert_eq!(result["products"], json!([]));
}

pub(super) fn malformed_nodes() -> Vec<Value> {
    let good = live(2, 10, "DENY");
    let mut replies = vec![
        json!({}),
        json!({"nodes":null}),
        json!({"nodes":false}),
        nodes(vec![]),
        nodes(vec![good.clone(), good.clone()]),
    ];
    for (path, value) in [
        (vec!["id"], json!(gid(3))),
        (vec!["id"], json!("gid://shopify/Order/2")),
        (vec!["__typename"], json!("Product")),
        (vec!["inventoryPolicy"], json!("UNKNOWN")),
        (vec!["inventoryQuantity"], Value::Null),
        (vec!["inventoryQuantity"], json!(1.2)),
        (vec!["inventoryQuantity"], json!("3")),
        (vec!["product", "id"], json!("bad")),
        (vec!["product", "status"], json!("UNKNOWN")),
        (vec!["image"], json!({})),
        (vec!["product", "featuredImage"], json!({})),
        (vec!["selectedOptions"], json!([{"name":"Size"}])),
    ] {
        let mut bad = good.clone();
        let mut cursor = &mut bad;
        for key in &path[..path.len() - 1] {
            cursor = &mut cursor[*key];
        }
        cursor[path[path.len() - 1]] = value;
        replies.push(nodes(vec![bad]));
    }
    for key in [
        "__typename",
        "id",
        "title",
        "sku",
        "image",
        "selectedOptions",
        "inventoryPolicy",
        "inventoryQuantity",
        "inventoryItem",
        "product",
    ] {
        let mut bad = good.clone();
        bad.as_object_mut().unwrap().remove(key);
        replies.push(nodes(vec![bad]));
    }
    for key in [
        "id",
        "title",
        "description",
        "productType",
        "featuredImage",
        "status",
    ] {
        let mut bad = good.clone();
        bad["product"].as_object_mut().unwrap().remove(key);
        replies.push(nodes(vec![bad]));
    }
    replies
}

#[tokio::test]
async fn malformed_complete_batch_never_becomes_partial_catalogue_or_quote() {
    let f = fixture().await;
    let (id, slug) = event(&f).await;
    select(&f, id, 2, "12.99").await;
    let replies = malformed_nodes();
    let mut both = Vec::new();
    for reply in &replies {
        both.push(reply.clone());
        both.push(reply.clone());
    }
    let (c, upstream) = common::client_with_upstream_script(both, "http").await;
    for _ in replies {
        for (suffix, method, body) in [
            ("products", "GET", None),
            ("quote", "POST", Some(quote(vec![line(2, 1)]))),
        ] {
            let error = response(&c, method, &format!("{URI}/{slug}/{suffix}"), body, 502).await;
            assert_eq!(error, json!({"error":"Upstream request failed"}));
        }
    }
    assert_ne!(upstream.requests().len(), 0);
}

#[tokio::test]
async fn batch_position_and_duplicate_identities_are_not_partial_success() {
    let f = fixture().await;
    let (id, slug) = event(&f).await;
    select(&f, id, 2, "12.99").await;
    select(&f, id, 3, "20").await;
    let duplicate = nodes(vec![live(2, 10, "DENY"), live(2, 10, "DENY")]);
    let reordered = nodes(vec![live(3, 10, "DENY"), live(2, 10, "DENY")]);
    let (c, _upstream) = common::client_with_upstream_script(
        vec![duplicate.clone(), duplicate, reordered.clone(), reordered],
        "http",
    )
    .await;
    for _ in 0..2 {
        response(&c, "GET", &format!("{URI}/{slug}/products"), None, 502).await;
        response(
            &c,
            "POST",
            &format!("{URI}/{slug}/quote"),
            Some(quote(vec![line(2, 1), line(3, 1)])),
            502,
        )
        .await;
    }
}

#[tokio::test]
async fn catalogue_payment_capabilities_follow_saved_method_updates() {
    let f = fixture().await;
    let (id, slug) = event(&f).await;
    let c = common::client().await;
    for methods in [vec!["stripe"], vec!["paypal"], vec!["stripe", "paypal"]] {
        sqlx::query("UPDATE conventions SET payment_methods = $1 WHERE id = $2")
            .bind(&methods)
            .bind(id)
            .execute(&f.pool())
            .await
            .unwrap();
        let body = get(&c, &format!("{URI}/{slug}/products")).await;
        assert_eq!(body["payment_methods"], json!(methods));
        assert!(body["convention"].get("payment_methods").is_none());
    }
}

#![allow(clippy::unwrap_used)]
use super::*;
use crate::client::shopify::ShopifyClient;
use crate::{
    client::tests::{Fixture, Reply},
    services::common::tests::{assert_failure, id, page, state},
};
use serde_json::{Value, json};

fn line(quantity: i64) -> Value {
    json!({
        "title":"Item","variantTitle":null,"sku":"GENSTSWOW01","quantity":quantity,
        "originalUnitPriceSet":{"shopMoney":{"amount":"0.1","currencyCode":"USD"}},
        "product":null,"variant":null
    })
}

fn order(n: u64, lines: Value) -> Value {
    let mut value = json!({
        "id":id("Order",n),"number":n,"tags":[],"email":null,"phone":null,
        "currentTotalPriceSet":{"shopMoney":{"amount":"1","currencyCode":"USD"}}
    });
    value["lineItems"] = lines;
    value
}

#[tokio::test]
async fn full_report_merges_matching_skus_across_all_order_pages() {
    let f = Fixture::new(vec![
        Reply::json(
            json!({"data":{"orders":page(vec![order(1,page(vec![line(2)],None))],Some("next"))}}),
        ),
        Reply::json(json!({"data":{"orders":page(vec![order(2,page(vec![line(3)],None))],None)}})),
    ])
    .await;
    let result = order_report(&state(&f), vec!["a:b".into()]).await.unwrap();
    assert_eq!(result.products.len(), 1);
    assert_eq!(result.products[0].quantity, 5);
    assert_eq!(result.products[0].total_selling_price.to_string(), "0.5");
    assert_eq!(
        serde_json::to_value(&result)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        1
    );
    let requests = f.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].2["variables"],
        json!({"search":r#"tag:"a\:b""#,"first":250,"after":null})
    );
    assert_eq!(requests[1].2["variables"]["after"], "next");
    assert!(requests.iter().all(|r| r.2["variables"]["first"] == 250));
}

#[tokio::test]
async fn missing_skus_stay_separate_in_first_seen_order_across_pages() {
    let mut missing = line(1);
    missing["sku"] = Value::Null;
    let f=Fixture::new(vec![
        Reply::json(json!({"data":{"orders":page(vec![order(1,page(vec![missing.clone(),line(2)],None))],Some("next"))}})),
        Reply::json(json!({"data":{"orders":page(vec![order(2,page(vec![line(3),missing],None))],None)}}))
    ]).await;
    let result = order_report(&state(&f), vec!["a:b".into()]).await.unwrap();
    assert_eq!(
        result
            .products
            .iter()
            .map(|r| (r.sku.as_deref(), r.quantity))
            .collect::<Vec<_>>(),
        vec![(None, 1), (Some("GENSTSWOW01"), 5), (None, 1)]
    );
}

#[tokio::test]
async fn later_upstream_failures_never_return_partial_reports() {
    for failure in [
        Reply::status(503),
        Reply::json(json!({"data":null,"errors":[{"message":"private"}]})),
        Reply {
            status: 200,
            body: "{".into(),
            headers: vec![],
        },
    ] {
        let f=Fixture::new(vec![Reply::json(json!({"data":{"orders":page(vec![order(1,page(vec![line(2)],None))],Some("next"))}})),failure]).await;
        let error = order_report(&state(&f), vec!["a:b".into()])
            .await
            .unwrap_err();
        assert_eq!(
            error.location().file(),
            "services/api/src/services/orders.rs"
        );
        assert_failure(&f, error, 2).await;
    }
}

#[tokio::test]
async fn nested_continuation_fails_without_a_child_lookup() {
    let f=Fixture::new(vec![Reply::json(json!({"data":{"orders":page(vec![order(1,page(vec![line(2)],Some("child")))],Some("next"))}}))]).await;
    assert_failure(
        &f,
        order_report(&state(&f), vec!["a:b".into()])
            .await
            .unwrap_err(),
        1,
    )
    .await;
}

#[tokio::test]
async fn exactly_250_nested_nodes_are_included() {
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"orders":page(vec![order(1,page(vec![line(1);250],None))],None)}}),
    )])
    .await;
    let result = order_report(&state(&f), vec!["a:b".into()]).await.unwrap();
    assert_eq!(result.products[0].quantity, 250);
    assert_eq!(result.products[0].total_selling_price.to_string(), "25");
}

#[tokio::test]
async fn empty_root_pages_continue_and_missing_cursors_fail() {
    let f = Fixture::new(vec![
        Reply::json(json!({"data":{"orders":page(vec![],Some("next"))}})),
        Reply::json(json!({"data":{"orders":page(vec![],None)}})),
    ])
    .await;
    assert_eq!(
        serde_json::to_value(order_report(&state(&f), vec!["a:b".into()]).await.unwrap()).unwrap(),
        json!({"products":[]})
    );
    assert_eq!(f.requests().len(), 2);
    for end in [Value::Null, json!("")] {
        let f = Fixture::new(vec![Reply::json(
            json!({"data":{"orders":{"nodes":[],"pageInfo":{"hasNextPage":true,"endCursor":end}}}}),
        )])
        .await;
        assert_failure(
            &f,
            order_report(&state(&f), vec!["a:b".into()])
                .await
                .unwrap_err(),
            1,
        )
        .await;
    }
}

#[tokio::test]
async fn service_resolves_unavailable_shared_dependency_without_network() {
    let f = Fixture::new(vec![]).await;
    let mut state = state(&f);
    state.shopify = Ok(None);
    let error = order_report(&state, vec!["a:b".into()]).await.unwrap_err();
    assert_eq!(
        axum::response::IntoResponse::into_response(error).status(),
        503
    );
    let source = ShopifyClient::new(&crate::config::ShopifyConfig {
        admin_api_url: "invalid-url".into(),
        timeout_secs: 3,
        access_token: Some("fixture".into()),
    })
    .err()
    .unwrap();
    state.shopify = Err(std::sync::Arc::new(source));
    let error = order_report(&state, vec!["a:b".into()]).await.unwrap_err();
    assert!(std::error::Error::source(&error).is_some());
    assert_eq!(
        axum::response::IntoResponse::into_response(error).status(),
        503
    );
    assert_eq!(f.requests().len(), 0);
}

#[tokio::test]
async fn later_numeric_failures_never_return_partial_reports() {
    for kind in ["quantity-overflow", "invalid-amount"] {
        let first = line(if kind == "quantity-overflow" {
            i64::MAX
        } else {
            1
        });
        let mut second = line(1);
        if kind == "invalid-amount" {
            second["originalUnitPriceSet"]["shopMoney"]["amount"] = json!("private-invalid-amount");
        }
        let f = Fixture::new(vec![
            Reply::json(
                json!({"data":{"orders":page(vec![order(1,page(vec![first],None))],Some("next"))}}),
            ),
            Reply::json(
                json!({"data":{"orders":page(vec![order(2,page(vec![second],None))],None)}}),
            ),
        ])
        .await;
        assert_failure(
            &f,
            order_report(&state(&f), vec!["a:b".into()])
                .await
                .unwrap_err(),
            2,
        )
        .await;
    }
}

#[tokio::test]
async fn shipping_label_edits_match_stored_tag_spelling_and_skip_existing_additions() {
    let order = json!({"id":"gid://shopify/Order/42","email":null,"totalWeight":100,"shippingAddress":{"firstName":"A","lastName":"B","name":"A B","address1":"1 Test","address2":null,"city":"City","provinceCode":"TS","zip":"123","country":"USA","countryCodeV2":"US","phone":"123"},"shippingLines":{"nodes":[{"code":"Standard Shipping"}]},"lineItems":{"nodes":[{"title":"Item","sku":"ABCKEY000001","quantity":1,"originalUnitPriceSet":{"shopMoney":{"amount":"30","currencyCode":"USD"}}}]}});
    let tags = json!({"data":{"orders":page(vec![json!({"id":id("Order",42),"name":"#42","tags":["sHiPpInG","fastship","Packing","packing","Other","อื่น"]})],None)}});
    let fixture = Fixture::new(vec![
        Reply::json(json!({"data":{"orders":page(vec![order],None)}})),
        Reply::json(tags.clone()),
        Reply::json(tags),
        Reply::json(json!({"data":{"tagsRemove":{"node":{"id":id("Order",42)},"userErrors":[]}}})),
    ])
    .await;
    let result = shipping_label::labels(&state(&fixture)).await;
    assert!(result.is_ok(), "label flow failed");
    let requests = fixture.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests[1].2["variables"],
        json!({"first":1,"after":null,"search":"id:42","includeContact":false,"includeTags":true})
    );
    assert_eq!(
        requests[3].2["variables"]["tags"],
        json!(["Packing", "packing"])
    );
    assert_eq!(requests[3].2["variables"]["add"], false);
}

#[tokio::test]
async fn tag_edits_only_mutate_case_insensitive_differences() {
    use crate::client::shopify::TagAction;
    for (stored, requested, add, expected) in [
        (
            vec!["Returned", "Other", "อื่น"],
            vec!["RETURNED"],
            true,
            vec![],
        ),
        (
            vec!["Other"],
            vec!["SHIPPING", "shipping", "FASTSHIP", "fastship"],
            true,
            vec!["SHIPPING", "FASTSHIP"],
        ),
        (
            vec!["shipping", "sHiPpInG", "Other", "อื่น"],
            vec!["SHIPPING"],
            false,
            vec!["shipping", "sHiPpInG"],
        ),
        (vec!["Other", "อื่น"], vec!["PACKING"], false, vec![]),
        (vec![], vec![], true, vec![]),
    ] {
        let mut replies = vec![Reply::json(
            json!({"data":{"orders":page(vec![json!({"id":id("Order",42),"name":"#42","tags":stored})],None)}}),
        )];
        if !expected.is_empty() {
            replies.push(Reply::json(json!({"data":{if add {"tagsAdd"} else {"tagsRemove"}:{"node":{"id":id("Order",42)},"userErrors":[]}}})));
        }
        let fixture = Fixture::new(replies).await;
        let state = state(&fixture);
        let client = shopify_client(&state).unwrap();
        edit_order_tags(
            client,
            "gid://shopify/Order/42",
            &requested,
            if add {
                TagAction::Add
            } else {
                TagAction::Remove
            },
        )
        .await
        .unwrap();
        let requests = fixture.requests();
        assert_eq!(requests.len(), 1 + usize::from(!expected.is_empty()));
        if !expected.is_empty() {
            assert_eq!(requests[1].2["variables"]["tags"], json!(expected));
        }
    }
}

#[tokio::test]
async fn listing_returns_customer_full_names_and_order_contact_email() {
    let fixture = Fixture::new(vec![Reply::json(json!({"data":{"orders":page(vec![
        json!({"id":id("Order",42),"number":1085,"shippingAddress":null,"email":"order@example.test","customer":{"firstName":" Jane ","lastName":" Doe "}}),
        json!({"id":id("Order",43),"number":1086,"shippingAddress":null,"email":null,"customer":null}),
        json!({"id":id("Order",44),"number":1087,"shippingAddress":null,"email":"guest@example.test","customer":{"firstName":"  ","lastName":"นามสกุล"}}),
        json!({"id":id("Order",45),"number":1088,"shippingAddress":null,"email":null,"customer":{"firstName":"ชื่อ","lastName":null}}),
        json!({"id":id("Order",46),"number":1089,"shippingAddress":null,"email":null,"customer":{"firstName":null,"lastName":" "}})
    ],Some("next"))}}))]).await;
    let response = list(
        &state(&fixture),
        "tag:shipping".into(),
        PageRequest {
            first: 5,
            after: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(response).unwrap(),
        json!({"orders":[
        {"id":42,"number":1085,"shipping_country_code":null,"full_name":"Jane Doe","email":"order@example.test"},
        {"id":43,"number":1086,"shipping_country_code":null,"full_name":null,"email":null},
        {"id":44,"number":1087,"shipping_country_code":null,"full_name":"นามสกุล","email":"guest@example.test"},
        {"id":45,"number":1088,"shipping_country_code":null,"full_name":"ชื่อ","email":null},
        {"id":46,"number":1089,"shipping_country_code":null,"full_name":null,"email":null}
    ],"page_info":{"has_next_page":true,"end_cursor":"next"}})
    );
    assert_eq!(fixture.requests()[0].2["variables"]["includeContact"], true);
}

#[tokio::test]
async fn listing_returns_shipping_country_code_when_address_exists() {
    let fixture=Fixture::new(vec![Reply::json(json!({"data":{"orders":page(vec![
        json!({"id":id("Order",1),"number":1085,"email":null,"customer":null,"shippingAddress":{"countryCodeV2":"TH"}}),
        json!({"id":id("Order",2),"number":1086,"email":null,"customer":null,"shippingAddress":null}),
        json!({"id":id("Order",3),"number":1087,"email":null,"customer":null,"shippingAddress":{"countryCodeV2":null}})
    ],None)}}))]).await;
    let response = list(
        &state(&fixture),
        String::new(),
        PageRequest {
            first: 3,
            after: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        serde_json::to_value(response).unwrap(),
        json!({"orders":[
        {"id":1,"number":1085,"full_name":null,"email":null,"shipping_country_code":"TH"},
        {"id":2,"number":1086,"full_name":null,"email":null,"shipping_country_code":null},
        {"id":3,"number":1087,"full_name":null,"email":null,"shipping_country_code":null}
    ],"page_info":{"has_next_page":false,"end_cursor":null}})
    );
}

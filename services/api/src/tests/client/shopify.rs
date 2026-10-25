#![allow(clippy::unwrap_used, clippy::needless_pass_by_value)]
use super::*;
use crate::client::tests::{Fixture, Reply};
use crate::shopify_models::{PageInfo, ProductStatus};
use serde_json::{Value, json};
use uuid::Uuid;
fn id(kind: &str, n: u64) -> ShopifyId {
    ShopifyId::new(format!("gid://shopify/{kind}/{n}")).unwrap()
}
fn client(f: &Fixture) -> ShopifyClient {
    ShopifyClient::new(&ShopifyConfig {
        admin_api_url: f.url.to_string(),
        timeout_secs: 3,
        access_token: Some("private-token".into()),
    })
    .unwrap()
}
fn page(nodes: Vec<Value>, cursor: Option<&str>) -> Value {
    json!({"nodes":nodes,"pageInfo":{"hasNextPage":cursor.is_some(),"endCursor":cursor}})
}
fn money() -> Value {
    json!({"amount":"123.4500","currencyCode":"THB"})
}
fn variant(_n: u64) -> Value {
    json!({"title":"M","sku":null,"contextualPricing":{"price":money()},"inventoryItem":{"unitCost":null,"measurement":{"weight":null},"inventoryLevel":null}})
}
fn product(_n: u64, variants: Vec<Value>) -> Value {
    json!({"title":"Shirt","productType":"KEYCHAIN","variants":page(variants,None)})
}
fn order(n: u64) -> Value {
    json!({"id":format!("gid://shopify/Order/{n}"),"name":"#1","number":1,"createdAt":"2026-10-01T00:00:00+07:00",
        "displayFinancialStatus":"PENDING","displayFulfillmentStatus":"UNFULFILLED","tags":[],
        "email":null,"phone":null,"shippingAddress":null,
        "currentTotalPriceSet":{"shopMoney":money()},"transactions":[], "lineItems":page(vec![],None), "shippingLines":page(vec![],None), "refunds":[]})
}
fn inventory(n: u64, active: bool, on_hand: i64, tracked: bool) -> Value {
    json!({"id":format!("level-{n}"),"isActive":active,
        "quantities":[{"name":"on_hand","quantity":on_hand},{"name":"available","quantity":on_hand}],
        "item":{"id":format!("gid://shopify/InventoryItem/{n}"),"sku":null,"tracked":tracked,
        "variant":{"id":format!("gid://shopify/ProductVariant/{n}"),"title":"M","product":{"id":"gid://shopify/Product/1","title":"Shirt"}}}})
}
fn tagged_order(_n: u64, lines: Vec<Value>) -> Value {
    json!({"lineItems":page(lines,None)})
}
fn tagged_line() -> Value {
    json!({"title":"Shirt","variantTitle":"Blue","sku":"SHIRT-BLUE","quantity":3,
        "originalUnitPriceSet":{"shopMoney":money()},"product":null,"variant":null})
}
#[test]
fn constructor_trusts_configuration_but_rejects_invalid_url_or_token() {
    let config = ShopifyConfig {
        admin_api_url: "http://localhost:9999/custom/graphql?version=1".into(),
        timeout_secs: 3,
        access_token: Some("token".into()),
    };
    assert!(ShopifyClient::new(&config).unwrap().headers["X-Shopify-Access-Token"].is_sensitive());
    assert!(
        ShopifyClient::new(&ShopifyConfig {
            timeout_secs: 0,
            ..config.clone()
        })
        .is_err()
    );
    assert!(
        ShopifyClient::new(&ShopifyConfig {
            admin_api_url: "not a URL".into(),
            ..config.clone()
        })
        .is_err()
    );
    assert!(
        ShopifyClient::new(&ShopifyConfig {
            access_token: None,
            ..config.clone()
        })
        .is_err()
    );
    assert!(
        ShopifyClient::new(&ShopifyConfig {
            access_token: Some("   ".into()),
            ..config.clone()
        })
        .is_err()
    );
    assert!(
        ShopifyClient::new(&ShopifyConfig {
            access_token: Some("bad\nheader".into()),
            ..config
        })
        .is_err()
    );
}

#[tokio::test]
async fn mutations_return_raw_user_errors_and_optional_outputs_without_replay() {
    let f=Fixture::new(vec![
        Reply::json(json!({"data":{"inventoryActivate":{"inventoryLevel":null,"userErrors":[{"field":null,"message":"denied"}]}}})),
    ]).await;
    let c = client(&f);
    let key = Uuid::new_v4();
    let activation = c
        .activate_existing_inventory(&ActivateInventoryRequest {
            inventory_item_id: id("InventoryItem", 1),
            location_id: id("Location", 1),
            idempotency_key: key,
        })
        .await
        .unwrap()
        .inventory_activate;
    assert!(activation.inventory_level.is_none());
    assert_eq!(activation.user_errors.len(), 1);
    let requests = f.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].2["variables"]["idempotencyKey"],
        key.to_string()
    );
    assert!(requests[0].2["variables"].get("available").is_none());
    assert!(requests[0].2["variables"].get("onHand").is_none());
}

#[tokio::test]
async fn activation_omits_quantities_and_preserves_idempotency_key_on_throttling() {
    let f=Fixture::new(vec![Reply::status(429),Reply::json(json!({"data":{"inventoryActivate":{
        "inventoryLevel":{"id":"level-1","isActive":true,"quantities":[{"name":"available","quantity":1},{"name":"on_hand","quantity":1}]},"userErrors":[]
    }}}))]).await;
    let key = Uuid::parse_str("79c99261-13df-45c7-818c-f24b111a75c9").unwrap();
    let level = client(&f)
        .activate_existing_inventory(&ActivateInventoryRequest {
            inventory_item_id: id("InventoryItem", 1),
            location_id: id("Location", 1),
            idempotency_key: key,
        })
        .await
        .unwrap()
        .inventory_activate
        .inventory_level
        .unwrap();
    assert!(level.is_active);
    assert_eq!(level.quantities[0].quantity, 1);
    let requests = f.requests();
    assert_eq!(
        requests[0].2["variables"]["idempotencyKey"],
        key.to_string()
    );
    assert_eq!(requests[0].2, requests[1].2);
    assert!(
        !requests[0].2["query"]
            .as_str()
            .unwrap()
            .contains("available:")
    );
    assert!(!requests[0].2["query"].as_str().unwrap().contains("onHand:"));
    for (_, _, request) in requests {
        let variables = request["variables"].as_object().unwrap();
        assert_eq!(variables.len(), 3);
        assert!(variables.get("onHand").is_none());
        assert!(variables.get("available").is_none());
        let query = request["query"].as_str().unwrap();
        assert!(!query.contains("$onHand") && !query.contains("$available"));
    }
}

#[tokio::test]
async fn location_lookup_has_no_catalog_pricing_dependency() {
    for exists in [false, true] {
        let location = if exists {
            json!({"id":id("Location",1)})
        } else {
            Value::Null
        };
        let f = Fixture::new(vec![Reply::json(json!({"data":{"location":location}}))]).await;
        assert_eq!(
            client(&f)
                .location_exists(&id("Location", 1))
                .await
                .unwrap(),
            exists
        );
        assert_eq!(f.requests().len(), 1);
        assert!(
            !f.requests()[0].2["query"]
                .as_str()
                .unwrap()
                .contains("contextualPricing")
        );
    }
}

#[tokio::test]
async fn proxy_returns_one_parent_page_without_following_root_or_child_cursors() {
    let mut first = product(1, vec![variant(1)]);
    first["variants"] = page(vec![variant(1)], Some("child-next"));
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"products":page(vec![first],Some("parent-next"))}}),
    )])
    .await;
    let result = client(&f)
        .products_with_variants(
            &ListProductsRequest {
                location_id: id("Location", 1),
                statuses: vec![ProductStatus::Active],
            },
            &request_page(),
        )
        .await
        .unwrap();
    assert_eq!(result.nodes.len(), 1);
    assert_eq!(result.page_info.end_cursor.as_deref(), Some("parent-next"));
    assert!(result.nodes[0].variants.page_info.has_next_page);
    let requests = f.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].2["variables"],
        json!({"locationId":id("Location",1),"search":"status:active","first":7,"after":"incoming"})
    );
}

#[tokio::test]
async fn tag_lookup_escapes_each_literal_and_requires_every_tag() {
    for (tags, search) in [
        (vec!["shipping"], r#"tag:"shipping""#),
        (vec![r#"a"b\c:(d)"#], r#"tag:"a\"b\\c\:\(d\)""#),
        (vec!["SHIPPING", "TEST"], r#"tag:"SHIPPING" AND tag:"TEST""#),
    ] {
        let f = Fixture::new(vec![Reply::json(
            json!({"data":{"orders":page(vec![],None)}}),
        )])
        .await;
        assert!(
            client(&f)
                .orders_by_tag(
                    &OrdersByTagRequest {
                        tags: tags.into_iter().map(str::to_owned).collect()
                    },
                    &request_page()
                )
                .await
                .unwrap()
                .nodes
                .is_empty()
        );
        assert_eq!(f.requests()[0].2["variables"]["search"], search);
    }
}

#[tokio::test]
async fn proxy_preserves_typed_upstream_failures() {
    for (kind, reply) in [
        ("http", Reply::status(503)),
        (
            "graphql",
            Reply::json(json!({"data":null,"errors":[{"message":"private"}]})),
        ),
        (
            "decode",
            Reply {
                status: 200,
                body: "{".into(),
                headers: vec![],
            },
        ),
    ] {
        let f = Fixture::new(vec![reply]).await;
        let error = client(&f)
            .orders_by_tag(
                &OrdersByTagRequest {
                    tags: vec!["tag".into()],
                },
                &request_page(),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error.location().file(), "services/api/src/client.rs");
        assert!(matches!(
            (kind, error.kind()),
            ("http", ClientErrorKind::Http { status: 503, .. })
                | ("graphql", ClientErrorKind::Graphql { .. })
                | ("decode", ClientErrorKind::Decode { .. })
        ));
    }
}
#[tokio::test]
async fn bounded_product_query_uses_fixed_250_variants() {
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"products":page(vec![],None)}}),
    )])
    .await;
    client(&f)
        .products_with_variants(
            &ListProductsRequest {
                location_id: id("Location", 1),
                statuses: vec![ProductStatus::Active],
            },
            &request_page(),
        )
        .await
        .unwrap();
    let requests = f.requests();
    let query = requests[0].2["query"].as_str().unwrap();
    assert!(query.contains("variants(first: 250)"));
    assert!(requests[0].2["variables"].get("variantsFirst").is_none());
}

fn request_page() -> PageRequest {
    PageRequest {
        first: 7,
        after: Some("incoming".into()),
    }
}
#[tokio::test]
async fn root_proxies_forward_pages_without_collecting_them() {
    for operation in ["OrdersByPayment", "OrdersByTag", "FindInventoryToActivate"] {
        let nodes = match operation {
            "OrdersByPayment" => vec![order(1)],
            "OrdersByTag" => vec![tagged_order(1, vec![tagged_line()])],
            _ => vec![inventory(1, false, 0, true)],
        };
        let connection = page(nodes, Some("next"));
        let response = if operation == "FindInventoryToActivate" {
            json!({"data":{"location":{"inventoryLevels":connection}}})
        } else {
            json!({"data":{"orders":connection}})
        };
        let f = Fixture::new(vec![Reply::json(response)]).await;
        let c = client(&f);
        let p = request_page();
        let (count, info) = call_root_proxy(&c, operation, &p).await;
        assert_eq!(count, 1);
        assert_eq!(info.end_cursor.as_deref(), Some("next"));
        assert!(info.has_next_page);
        let requests = f.requests();
        assert_eq!(requests.len(), 1);
        let request = &requests[0].2;
        let variables = &request["variables"];
        assert_eq!(request["operationName"], operation);
        assert_eq!(variables["first"], 7);
        assert_eq!(variables["after"], "incoming");
        assert!(variables.get("linesFirst").is_none());
        if operation == "OrdersByTag" {
            assert!(
                request["query"]
                    .as_str()
                    .unwrap()
                    .contains("lineItems(first: 250)")
            );
        }
    }
}
#[tokio::test]
async fn invalid_client_inputs_are_rejected_before_network() {
    let f = Fixture::new(vec![]).await;
    let c = client(&f);
    let p = request_page();
    for error in [
        c.orders_by_tag(
            &OrdersByTagRequest {
                tags: vec![" ".into()],
            },
            &p,
        )
        .await
        .err()
        .unwrap(),
        c.accounting_orders(
            "",
            &PageRequest {
                first: 0,
                after: None,
            },
        )
        .await
        .err()
        .unwrap(),
    ] {
        assert!(matches!(error.kind(), ClientErrorKind::InvalidInput(_)));
    }
    assert_eq!(f.requests().len(), 0);
}
async fn call_root_proxy(
    c: &ShopifyClient,
    operation: &str,
    page: &PageRequest,
) -> (usize, PageInfo) {
    match operation {
        "OrdersByPayment" => {
            let p = c
                .accounting_orders("payment_id:\"pay\"", page)
                .await
                .unwrap();
            (p.nodes.len(), p.page_info)
        }
        "OrdersByTag" => {
            let p = c
                .orders_by_tag(
                    &OrdersByTagRequest {
                        tags: vec!["shipping".into()],
                    },
                    page,
                )
                .await
                .unwrap();
            assert_eq!(p.nodes[0].line_items.nodes.len(), 1);
            (p.nodes.len(), p.page_info)
        }

        _ => {
            let p = c
                .find_inventory_to_activate(
                    &InventoryDiscoveryRequest {
                        location_id: id("Location", 1),
                    },
                    page,
                )
                .await
                .unwrap();
            let p = p.unwrap();
            assert!(!p.nodes[0].is_active);
            (p.nodes.len(), p.page_info)
        }
    }
}

#[tokio::test]
async fn inventory_discovery_distinguishes_missing_location_key_from_null() {
    let f = Fixture::new(vec![
        Reply::json(json!({"data":{}})),
        Reply::json(json!({"data":{"location":null}})),
    ])
    .await;
    let c = client(&f);
    let input = InventoryDiscoveryRequest {
        location_id: id("Location", 1),
    };
    let page = PageRequest {
        first: 250,
        after: None,
    };
    assert!(c.find_inventory_to_activate(&input, &page).await.is_err());
    assert!(
        c.find_inventory_to_activate(&input, &page)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(f.requests().len(), 2);
}

#[tokio::test]
async fn list_orders_explicitly_supplies_required_selection_flags() {
    let fixture = Fixture::new(vec![Reply::json(
        json!({"data":{"orders":page(vec![],None)}}),
    )])
    .await;
    client(&fixture)
        .list_orders(
            "tag:shipping AND fulfillment_status:unfulfilled",
            &PageRequest {
                first: 1,
                after: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        fixture.requests()[0].2["variables"],
        json!({"first":1,"after":null,"search":"tag:shipping AND fulfillment_status:unfulfilled","includeContact":true,"includeTags":false})
    );
}

#[tokio::test]
async fn location_lookup_rejects_missing_resource_and_mismatched_ids() {
    for data in [
        json!({}),
        json!({"location":{}}),
        json!({"location":{"id":"gid://shopify/Location/2"}}),
    ] {
        let f = Fixture::new(vec![Reply::json(json!({"data":data}))]).await;
        assert!(
            client(&f)
                .location_exists(&id("Location", 1))
                .await
                .is_err()
        );
        assert_eq!(f.requests().len(), 1);
    }
}

fn draft_input() -> crate::models::shopify::CreateDraftOrderInput {
    use crate::models::shopify::{CreateDraftOrderInput, DraftOrderLineItemInput};
    CreateDraftOrderInput {
        line_items: vec![DraftOrderLineItemInput {
            variant_id: id("ProductVariant", 123),
            quantity: 2,
            price_override: None,
        }],
        tags: vec![" EXPLICIT ".into(), "ทดสอบ".into()],
        ..Default::default()
    }
}
fn draft_response() -> Value {
    json!({"data":{"draftOrderCreate":{"draftOrder":{
        "id":"gid://shopify/DraftOrder/42", "name":"#D42", "email":null, "phone":null,
        "tags":["EXPLICIT"], "reserveInventoryUntil":null
    }, "userErrors":[]}}})
}
#[tokio::test]
async fn draft_creation_preserves_customer_prices_tags_and_optional_fields() {
    use crate::models::shopify::{DraftOrderLineItemInput, MailingAddressInput, Money};
    let f = Fixture::new(vec![
        Reply::json(draft_response()),
        Reply::json(draft_response()),
    ])
    .await;
    let mut input = draft_input();
    input.line_items[0].price_override = Some(Money {
        amount: "1234567890.0123456789".into(),
        currency_code: "THB".into(),
    });
    input.email = Some("buyer@example.test".into());
    input.phone = Some("+66000000000".into());
    input.shipping_address = Some(MailingAddressInput {
        first_name: Some("วิชัย".into()),
        last_name: Some("ทดสอบ".into()),
        company: Some("Example".into()),
        address1: Some("1 Example Road".into()),
        address2: Some("Unit 2".into()),
        city: Some("Bangkok".into()),
        province_code: Some("TH-10".into()),
        country_code: Some("TH".into()),
        zip: Some("10000".into()),
        phone: input.phone.clone(),
    });
    input.billing_address = input.shipping_address.clone();
    input.presentment_currency_code = Some("THB".into());
    input.reserve_inventory_until = Some("2026-10-23T12:34:56Z".parse().unwrap());
    input.session_token = Some("caller-owned-token".into());
    input.line_items.push(DraftOrderLineItemInput {
        variant_id: id("ProductVariant", 124),
        quantity: 1,
        price_override: Some(Money {
            amount: "0.1000".into(),
            currency_code: "THB".into(),
        }),
    });
    let result = client(&f).create_draft_order(&input).await.unwrap();
    assert_eq!(
        result.draft_order.unwrap().id.as_ref(),
        "gid://shopify/DraftOrder/42"
    );
    assert!(result.user_errors.is_empty());
    client(&f).create_draft_order(&draft_input()).await.unwrap();
    let requests = f.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].0, "/graphql.json");
    assert_eq!(requests[0].1["x-shopify-access-token"], "private-token");
    assert!(!requests[0].1.contains_key("idempotency-key"));
    assert_eq!(requests[0].2["operationName"], "CreateDraftOrder");
    assert!(
        !requests[0].2["query"]
            .as_str()
            .unwrap()
            .contains("@idempotent")
    );
    assert_eq!(
        requests[0].2["variables"],
        json!({"input":{
            "lineItems":[
                {"variantId":id("ProductVariant",123),"quantity":2,"priceOverride":{"amount":"1234567890.0123456789","currencyCode":"THB"}},
                {"variantId":id("ProductVariant",124),"quantity":1,"priceOverride":{"amount":"0.1000","currencyCode":"THB"}}
            ],
            "tags":[" EXPLICIT ","ทดสอบ"], "email":"buyer@example.test", "phone":"+66000000000",
            "shippingAddress":{"firstName":"วิชัย","lastName":"ทดสอบ","company":"Example","address1":"1 Example Road","address2":"Unit 2","city":"Bangkok","provinceCode":"TH-10","countryCode":"TH","zip":"10000","phone":"+66000000000"},
            "billingAddress":serde_json::to_value(&input.billing_address).unwrap(),
            "presentmentCurrencyCode":"THB", "reserveInventoryUntil":"2026-10-23T12:34:56Z", "sessionToken":"caller-owned-token"
        }})
    );
    assert_eq!(
        requests[1].2["variables"],
        json!({"input":{
            "lineItems":[{"variantId":id("ProductVariant",123),"quantity":2}],
            "tags":[" EXPLICIT ","ทดสอบ"]
        }})
    );
    assert_eq!(f.methods(), vec![reqwest::Method::POST; 2]);
}
#[tokio::test]
async fn draft_creation_omits_price_override_for_catalogue_prices() {
    let fixture = Fixture::new(vec![Reply::json(draft_response())]).await;
    client(&fixture)
        .create_draft_order(&draft_input())
        .await
        .unwrap();
    let requests = fixture.requests();
    let line_items = requests[0].2["variables"]["input"]["lineItems"]
        .as_array()
        .unwrap();
    assert_eq!(line_items.len(), 1);
    assert_eq!(
        line_items[0]["variantId"],
        "gid://shopify/ProductVariant/123"
    );
    assert_eq!(line_items[0]["quantity"], 2);
    assert!(
        !line_items[0]
            .as_object()
            .unwrap()
            .contains_key("priceOverride")
    );
}
#[tokio::test]
async fn draft_creation_preserves_provider_rejections_without_interpreting_them() {
    let f = Fixture::new(vec![Reply::json(json!({"data":{"draftOrderCreate":{
        "draftOrder":null,"userErrors":[{"field":["input","lineItems","0","quantity"],"message":"Invalid quantity"}]
    }}}))]).await;
    let result = client(&f).create_draft_order(&draft_input()).await.unwrap();
    assert!(result.draft_order.is_none());
    assert_eq!(result.user_errors[0].message, "Invalid quantity");
    assert_eq!(
        result.user_errors[0].field.as_ref().unwrap()[1],
        "lineItems"
    );
    assert_eq!(f.requests().len(), 1);
}
#[tokio::test]
async fn draft_creation_rejects_wrong_variant_kind_before_network() {
    let f = Fixture::new(vec![]).await;
    let mut input = draft_input();
    input.line_items[0].variant_id = id("Product", 123);
    let error = client(&f).create_draft_order(&input).await.err().unwrap();
    assert!(matches!(error.kind(), ClientErrorKind::InvalidInput(_)));
    assert_eq!(f.requests().len(), 0);
}
#[tokio::test]
async fn draft_creation_never_replays_http_failures() {
    for status in [400, 401, 403, 404, 408, 409, 422, 429, 500, 502, 503, 504] {
        let f = Fixture::new(vec![Reply::status(status), Reply::json(draft_response())]).await;
        let error = client(&f)
            .create_draft_order(&draft_input())
            .await
            .err()
            .unwrap();
        assert!(
            matches!(error.kind(), ClientErrorKind::Http {status: actual,..} if *actual == status)
        );
        assert_eq!(f.requests().len(), 1);
    }
}
#[tokio::test]
async fn draft_creation_never_replays_graphql_or_invalid_payload_failures() {
    for (kind, reply) in [
        (
            "graphql",
            Reply::json(
                json!({"errors":[{"message":"private-token buyer@example.test","extensions":{"code":"THROTTLED"}}]}),
            ),
        ),
        (
            "graphql",
            Reply::json(json!({"data":{"draftOrderCreate":null},"errors":[{"message":"denied"}]})),
        ),
        (
            "decode",
            Reply {
                status: 200,
                body: "{".into(),
                headers: vec![],
            },
        ),
        ("protocol", Reply::json(json!({}))),
        ("decode", Reply::json(json!({"data":{}}))),
        (
            "decode",
            Reply::json(json!({"data":{"draftOrderCreate":{"userErrors":[]}}})),
        ),
        (
            "decode",
            Reply::json(json!({"data":{"draftOrderCreate":{"draftOrder":null}}})),
        ),
        (
            "decode",
            Reply::json(json!({"data":{"draftOrderCreate":null}})),
        ),
    ] {
        let f = Fixture::new(vec![reply, Reply::json(draft_response())]).await;
        let error = client(&f)
            .create_draft_order(&draft_input())
            .await
            .err()
            .unwrap();
        assert!(matches!(
            (kind, error.kind()),
            ("graphql", ClientErrorKind::Graphql { .. })
                | ("decode", ClientErrorKind::Decode { .. })
                | ("protocol", ClientErrorKind::Protocol(_))
        ));
        for diagnostic in [error.to_string(), format!("{error:?}")] {
            assert!(!diagnostic.contains("private-token"));
            assert!(!diagnostic.contains("buyer@example.test"));
        }
        assert_eq!(error.location().file(), "services/api/src/client.rs");
        assert_eq!(f.requests().len(), 1);
    }
}

#[tokio::test]
async fn draft_orders_reads_one_generic_page_with_full_purchase_evidence() {
    let draft = json!({
        "id":"gid://shopify/DraftOrder/42", "ready":true, "status":"OPEN", "order":null,
        "tags":["checkout:test"], "email":"private@example.test", "paymentTerms":null,
        "totalPriceSet":{"presentmentMoney":{"amount":"40.00","currencyCode":"SGD"}},
        "reserveInventoryUntil":"2035-01-01T00:00:00Z",
        "lineItems":{"nodes":[{"quantity":2,"variant":{"id":"gid://shopify/ProductVariant/7"}}],
            "pageInfo":{"hasNextPage":false,"endCursor":null}}
    });
    let f = Fixture::new(vec![Reply::json(
        json!({"data":{"draftOrders":page(vec![draft],Some("next"))}}),
    )])
    .await;
    let result = client(&f)
        .draft_orders(
            PageRequest {
                first: 250,
                after: Some("previous".into()),
            },
            Some("tag:'checkout:test'"),
            None,
        )
        .await
        .unwrap();
    assert_eq!(result.nodes[0].id.as_ref(), "gid://shopify/DraftOrder/42");
    assert_eq!(
        result.nodes[0].email.as_deref(),
        Some("private@example.test")
    );
    assert_eq!(
        result.nodes[0].line_items.as_ref().unwrap().nodes[0].quantity,
        2
    );
    assert!(result.page_info.has_next_page);
    let requests = f.requests();
    assert_eq!(requests[0].2["operationName"], "DraftOrders");
    assert_eq!(
        requests[0].2["variables"],
        json!({"first":250,"after":"previous","query":"tag:'checkout:test'"})
    );
}

#[tokio::test]
async fn draft_orders_rejects_incomplete_response_and_wrong_resource_identity() {
    for value in [
        json!({"data":{}}),
        json!({"data":{"draftOrders":{"nodes":[],"pageInfo":{"hasNextPage":true}}}}),
        json!({"data":{"draftOrders":page(vec![json!({"id":"gid://shopify/Order/42","ready":true,"status":"OPEN","order":null,"tags":[],"totalPriceSet":{"presentmentMoney":money()},"paymentTerms":null})],None)}}),
    ] {
        let f = Fixture::new(vec![Reply::json(value)]).await;
        assert!(
            client(&f)
                .draft_orders(
                    PageRequest {
                        first: 250,
                        after: None
                    },
                    None,
                    None
                )
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn manual_recording_omits_new_method_and_preserves_legacy_wire_without_replay() {
    for method in [None, Some("Stripe Card Payments")] {
        let fixture = Fixture::new(vec![Reply::json(json!({"data":{"orderCreateManualPayment":{"order":{"id":"gid://shopify/Order/1"},"userErrors":[]}}}))]).await;
        assert_eq!(
            client(&fixture)
                .record_checkout_payment(&id("Order", 1), method)
                .await
                .unwrap()
                .as_ref(),
            "gid://shopify/Order/1"
        );
        let requests = fixture.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].2["operationName"], "RecordCheckoutPayment");
        let mut expected = json!({"id":"gid://shopify/Order/1"});
        if let Some(method) = method {
            expected["paymentMethodName"] = json!(method);
        }
        assert_eq!(requests[0].2["variables"], expected);
    }
    let fixture = Fixture::new(vec![Reply::status(503)]).await;
    assert!(
        client(&fixture)
            .record_checkout_payment(&id("Order", 1), None)
            .await
            .is_err()
    );
    assert_eq!(fixture.requests().len(), 1);
}

#[tokio::test]
async fn order_attribute_update_encodes_input_and_requires_exact_response_identity() {
    let attributes = vec![crate::models::shopify::CustomAttribute {
        key: "checkout_id".into(),
        value: "00000000-0000-4000-8000-000000000001".into(),
    }];
    let fixture = Fixture::new(vec![Reply::json(
        json!({"data":{"orderUpdate":{"order":{"id":"gid://shopify/Order/1"},"userErrors":[]}}}),
    )])
    .await;
    assert_eq!(
        client(&fixture)
            .update_order_custom_attributes(&id("Order", 1), &attributes)
            .await
            .unwrap()
            .as_ref(),
        "gid://shopify/Order/1"
    );
    assert_eq!(
        fixture.requests()[0].2["variables"],
        json!({"input":{"id":"gid://shopify/Order/1","customAttributes":attributes}})
    );
    for payload in [
        json!({"order":null,"userErrors":[]}),
        json!({"order":{"id":"gid://shopify/Order/2"},"userErrors":[]}),
        json!({"order":{"id":"gid://shopify/Product/1"},"userErrors":[]}),
        json!({"order":null,"userErrors":[{"field":["customAttributes"],"message":"rejected"}]}),
    ] {
        let fixture =
            Fixture::new(vec![Reply::json(json!({"data":{"orderUpdate":payload}}))]).await;
        assert!(
            client(&fixture)
                .update_order_custom_attributes(&id("Order", 1), &attributes)
                .await
                .is_err()
        );
    }
    let fixture = Fixture::new(vec![]).await;
    assert!(
        client(&fixture)
            .update_order_custom_attributes(&id("Product", 1), &attributes)
            .await
            .is_err()
    );
    assert_eq!(fixture.requests().len(), 0);
}

#[tokio::test]
async fn draft_recovery_created_sort_is_explicit_and_keeps_fixed_search() {
    let fixture = Fixture::new(vec![Reply::json(
        json!({"data":{"draftOrders":page(vec![],None)}}),
    )])
    .await;
    let query = "created_at:>='2026-10-25T00:00:00+00:00' created_at:<='2026-10-25T00:10:00+00:00'";
    client(&fixture)
        .draft_orders(
            PageRequest {
                first: 250,
                after: None,
            },
            Some(query),
            Some(crate::models::shopify::DraftOrderSortKey::CreatedAt),
        )
        .await
        .unwrap();
    assert_eq!(
        fixture.requests()[0].2["variables"],
        json!({"first":250,"after":null,"query":query,"sortKey":"CREATED_AT"})
    );
}

#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    client::tests::{Fixture, Reply},
    services::{checkout::tests::checkout, common::client},
};
use serde_json::json;

#[test]
fn purchase_tags_fit_shopify_limit_and_preserve_short_convention_names() {
    let convention_id = Uuid::new_v4();
    let checkout_id = Uuid::new_v4();
    let short = "Convention 2026";
    let tags = legacy_purchase_tags(convention_id, short, checkout_id, Some("test-convention"));
    assert_eq!(
        tags,
        vec![
            format!("cv:{convention_id}"),
            format!("name:{short}"),
            format!("co:{checkout_id}"),
            "slug:test-convention".to_owned()
        ]
    );
    assert!(tags.iter().all(|tag| tag.chars().count() <= 40));
    for name in ["A".repeat(120), "งานไทย🦊".repeat(20)] {
        let tags = legacy_purchase_tags(convention_id, &name, checkout_id, Some("test-convention"));
        assert_eq!(
            tags[1],
            format!("name:{}", name.chars().take(35).collect::<String>())
        );
        assert_eq!(tags[1].chars().count(), 40);
        assert!(tags.iter().all(|tag| tag.chars().count() <= 40));
        let mut row = checkout();
        row.required_tag_hashes = tags
            .iter()
            .map(|tag| Sha256::digest(tag.as_bytes()).to_vec())
            .collect();
        assert!(tags_match(&row, &tags));
        let mut changed = tags;
        changed[1].push('X');
        assert!(!tags_match(&row, &changed));
    }
}

#[tokio::test]
async fn recovery_search_uses_only_exact_correlation_bound_to_original_purchase() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@127.0.0.1:1/fixture")
        .unwrap();
    pool.close().await;
    for prefix in ["co", "checkout"] {
        let mut row = checkout();
        row.draft_id = None;
        let tag = format!("{prefix}:{}", row.id);
        row.required_tag_hashes = vec![Sha256::digest(tag.as_bytes()).to_vec()];
        let shop = Fixture::new(vec![Reply::json(json!({"data":{"draftOrders":{
            "nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}
        }}}))])
        .await;
        let shopify = client(&shop);
        let clients = CheckoutClients {
            shopify: &shopify,
            stripe: None,
            paypal: None,
        };
        assert!(
            recover(&pool, &clients, &mut row, Uuid::new_v4())
                .await
                .is_err()
        );
        let requests = shop.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].2["operationName"], "DraftOrders");
        assert_eq!(requests[0].2["variables"]["query"], format!("tag:'{tag}'"));
        assert!(row.draft_id.is_none());
    }
    let mut row = checkout();
    row.draft_id = None;
    row.required_tag_hashes = vec![Sha256::digest(b"co:foreign").to_vec()];
    let shop = Fixture::new(vec![]).await;
    let shopify = client(&shop);
    let clients = CheckoutClients {
        shopify: &shopify,
        stripe: None,
        paypal: None,
    };
    assert!(recovery_tag(&row).is_none());
    assert!(
        recover(&pool, &clients, &mut row, Uuid::new_v4())
            .await
            .is_err()
    );
    assert_eq!(shop.requests().len(), 0);
}

#[test]
fn v2_purchase_hash_binds_exact_slug_and_legacy_hash_remains_unchanged() {
    let lines = BTreeMap::from([("gid://shopify/ProductVariant/2".into(), 1)]);
    let money = domain::dto::checkout::CheckoutMoney {
        minor: 400,
        currency: "AUD".into(),
    };
    let old = evidence_hash("qa@example.test", &money, &lines).unwrap();
    assert_eq!(
        old,
        bound_evidence_hash("qa@example.test", &money, &lines, None).unwrap()
    );
    let new =
        bound_evidence_hash("qa@example.test", &money, &lines, Some("convention-one")).unwrap();
    assert_ne!(old, new);
    assert_ne!(
        new,
        bound_evidence_hash("qa@example.test", &money, &lines, Some("convention-two")).unwrap()
    );
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "Keep owned legacy row, identical replays and cleanup together"
)]
async fn stored_legacy_pre_reservation_replay_preserves_hashes_and_reaches_stock_again() {
    use crate::{config::StripeConfig, repositories::checkout as checkout_repo};
    let url = std::env::var("E2E_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into());
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let mut row = checkout();
    row.draft_state = DraftState::NotStarted;
    row.payment_state = PaymentState::NotStarted;
    row.draft_id = None;
    row.provider_resource_id = None;
    row.reservation_expires_at = None;
    let slug = format!("legacy-replay-{}", Uuid::new_v4().simple());
    sqlx::query(
        "INSERT INTO conventions (
                     id, slug, name, country_code, event_starts_on, event_ends_on,
                     description_markdown, preorder_enabled, currency_code, payment_methods
                 )
                 VALUES ($1, $2, 'Legacy replay', 'SG', DATE '2030-01-01', DATE '2030-01-01',
                         'Owned legacy replay fixture', TRUE, 'SGD', ARRAY['stripe'])",
    )
    .bind(row.convention_id)
    .bind(&slug)
    .execute(&pool)
    .await
    .unwrap();
    let variant = Uuid::new_v4();
    sqlx::query("INSERT INTO convention_variants (
                     id, convention_id, shopify_product_id, shopify_variant_id, final_price
                 )
                 VALUES ($1, $2, 'gid://shopify/Product/1', 'gid://shopify/ProductVariant/2', 20.00)")
        .bind(variant).bind(row.convention_id).execute(&pool).await.unwrap();
    let request:CreateCheckoutRequest=serde_json::from_value(json!({
        "operation_id":row.id.to_string(),"convention_id":row.convention_id.to_string(),"email":"private@example.test","provider":"stripe",
        "items":[{"variant_id":variant.to_string(),"quantity":2,"expected_unit_price":2000}],"expected_currency":"SGD"
    })).unwrap();
    row.request_fingerprint = Sha256::digest(serde_json::to_vec(&request).unwrap()).to_vec();
    row.required_tag_hashes = [
        format!("cv:{}", row.convention_id),
        "Legacy replay".into(),
        format!("co:{}", row.id),
    ]
    .iter()
    .map(|tag| Sha256::digest(tag.as_bytes()).to_vec())
    .collect();
    checkout_repo::insert(&mut pool.acquire().await.unwrap(), &row)
        .await
        .unwrap();
    row = checkout_repo::find(&pool, row.id).await.unwrap().unwrap();
    let shop = Fixture::new(vec![Reply::status(500); 20]).await;
    let gateway = Fixture::new(vec![
        Reply::json(
            json!({"id":"acct_test","object":"account"})
        );
        2
    ])
    .await;
    let stripe = crate::client::stripe::StripeClient::new(&StripeConfig {
        checkout_api_version: "2026-08-26.dahlia".into(),
        base_url: gateway.url.join("/v1/").unwrap().to_string(),
        timeout_secs: 1,
        secret_key: Some("fixture".into()),
    })
    .unwrap();
    let shopify = client(&shop);
    let clients = CheckoutClients {
        shopify: &shopify,
        stripe: Some(&stripe),
        paypal: None,
    };
    let settings = config::Config::builder()
        .add_source(config::File::from_str(
            include_str!("../../../../application.toml"),
            config::FileFormat::Toml,
        ))
        .add_source(config::File::from_str(
            include_str!("../../../../application.qa.toml"),
            config::FileFormat::Toml,
        ))
        .build()
        .unwrap()
        .get::<CheckoutSettings>("checkout")
        .unwrap();
    let mut errors = Vec::new();
    for _ in 0..2 {
        errors.push(
            axum::response::IntoResponse::into_response(
                super::super::create(&pool, &settings, &clients, &request, &[17; 32])
                    .await
                    .unwrap_err(),
            )
            .status(),
        );
    }
    let stored = checkout_repo::find(&pool, row.id).await.unwrap().unwrap();
    sqlx::query(
        "DELETE FROM checkouts
                 WHERE id = $1",
    )
    .bind(row.id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "DELETE FROM conventions
                 WHERE id = $1",
    )
    .bind(row.convention_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(errors, vec![axum::http::StatusCode::BAD_GATEWAY; 2]);
    assert!(shop.requests().len() >= 2);
    assert!(
        shop.requests()
            .iter()
            .all(|request| request.2["operationName"] == "Location")
    );
    let requests = gateway.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| request.0 == "/v1/account"));
    assert_eq!(stored.completion_version, 1);
    assert!(stored.convention_slug.is_none());
    assert_eq!(stored.request_fingerprint, row.request_fingerprint);
    assert_eq!(stored.owner_expires_at, row.owner_expires_at);
    assert_eq!(stored.evidence_hash, row.evidence_hash);
    assert_eq!(stored.required_tag_hashes, row.required_tag_hashes);
    assert!(stored.draft_id.is_none());
    assert_eq!(stored.draft_state, DraftState::NotStarted);
}

#[test]
fn v2_name_and_slug_tags_survive_shopify_punctuation_equivalence() {
    let convention = Uuid::new_v4();
    let checkout = Uuid::new_v4();
    let slug = "qa-checkout-59b5fcc9";
    for name in [
        "QA checkout 59b5fcc9",
        "qa_checkout_59b5fcc9",
        "slug qa checkout 59b5fcc9",
    ] {
        let tags = legacy_purchase_tags(convention, name, checkout, Some(slug));
        let canonical: BTreeSet<String> = tags
            .iter()
            .map(|tag| {
                tag.chars()
                    .filter(|ch| ch.is_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect()
            })
            .collect();
        assert_eq!(canonical.len(), 4, "Shopify would collapse {tags:?}");
        assert_eq!(tags[1], format!("name:{name}"));
        assert_eq!(tags[3], format!("slug:{slug}"));
    }
}

#[test]
fn v2_slug_namespace_respects_35_36_40_41_and_63_character_boundaries() {
    let convention = Uuid::new_v4();
    let checkout = Uuid::new_v4();
    for length in [35, 36, 40, 41, 63] {
        let slug = "a".repeat(length);
        let tags = legacy_purchase_tags(convention, &"N".repeat(63), checkout, Some(&slug));
        assert!(tags.iter().all(|tag| tag.chars().count() <= 40));
        assert_eq!(tags[1], format!("name:{}", "N".repeat(35)));
        if length <= 35 {
            assert_eq!(tags[3], format!("slug:{slug}"));
        } else {
            assert_eq!(tags[3].chars().count(), 37);
            let distinct = format!("{}b", "a".repeat(length - 1));
            assert_ne!(
                tags[3],
                legacy_purchase_tags(convention, "Convention", checkout, Some(&distinct))[3]
            );
        }
    }
}

#[test]
fn legacy_name_and_tags_remain_byte_for_byte_unchanged() {
    let convention = Uuid::new_v4();
    let checkout = Uuid::new_v4();
    for name in ["QA checkout 59b5fcc9".to_owned(), "งานไทย🦊".repeat(20)] {
        assert_eq!(
            legacy_purchase_tags(convention, &name, checkout, None),
            vec![
                format!("cv:{convention}"),
                name.chars().take(40).collect::<String>(),
                format!("co:{checkout}")
            ]
        );
    }
}

#[test]
fn new_purchase_exposes_only_convention_slug_tag() {
    let tags = purchase_tags(
        Uuid::new_v4(),
        "Same Convention",
        Uuid::new_v4(),
        Some("same-convention"),
    );
    assert_eq!(tags, vec!["same-convention"]);
}

fn metadata_checkout() -> Checkout {
    let mut row = checkout();
    row.completion_version = 2;
    row.correlation_version = correlation::METADATA_VERSION;
    row.convention_slug = Some("qa-checkout-59b5fcc9".into());
    let lines = BTreeMap::from([("gid://shopify/ProductVariant/2".into(), 2)]);
    row.evidence_hash = bound_evidence_hash(
        "private@example.test",
        &super::super::tests::money(),
        &lines,
        row.convention_slug.as_deref(),
    )
    .unwrap();
    row.required_tag_hashes = vec![Sha256::digest(b"qa-checkout-59b5fcc9").to_vec()];
    row
}

fn metadata_draft(row: &Checkout) -> serde_json::Value {
    let mut draft = super::super::tests::draft(row);
    draft["customAttributes"] =
        serde_json::to_value(correlation::identity_attributes(row).unwrap()).unwrap();
    draft["tags"] = json!([row
        .convention_slug
        .as_ref()
        .unwrap()
        .chars()
        .take(40)
        .collect::<String>()]);
    draft["createdAt"] = json!(row.created_at);
    draft
}

#[test]
fn metadata_purchase_requires_identity_and_single_slug_but_preserves_purchase_hash() {
    let row = metadata_checkout();
    let value = metadata_draft(&row);
    let draft: CompletionDraft = serde_json::from_value(value.clone()).unwrap();
    assert!(validate_purchase(&row, &draft).is_ok());
    let mut changed = value.clone();
    changed["tags"] = json!([row.convention_slug, row.convention_slug]);
    assert!(validate_purchase(&row, &serde_json::from_value(changed).unwrap()).is_err());
    for key in [
        correlation::CHECKOUT_KEY,
        correlation::CONVENTION_KEY,
        correlation::PROVIDER_KEY,
        correlation::VERSION_KEY,
        correlation::SLUG_KEY,
    ] {
        let mut changed = value.clone();
        changed["customAttributes"]
            .as_array_mut()
            .unwrap()
            .retain(|attribute| attribute["key"] != key);
        assert!(
            validate_purchase(&row, &serde_json::from_value(changed).unwrap()).is_err(),
            "{key}"
        );
    }
    let mut changed = value;
    changed["lineItems"]["nodes"][0]["quantity"] = json!(1);
    assert!(validate_purchase(&row, &serde_json::from_value(changed).unwrap()).is_err());
}

#[test]
fn new_slug_tags_preserve_full_40_41_63_character_identity() {
    for length in [1, 40, 41, 63] {
        let slug = "a".repeat(length);
        assert_eq!(
            purchase_tags(Uuid::new_v4(), "Name", Uuid::new_v4(), Some(&slug)),
            vec!["a".repeat(length.min(40))]
        );
        let mut row = metadata_checkout();
        row.convention_slug = Some(slug.clone());
        let attrs = correlation::identity_attributes(&row).unwrap();
        assert_eq!(
            correlation::unique_value(&attrs, correlation::SLUG_KEY).unwrap(),
            Some(slug.as_str())
        );
        let visible: String = slug.chars().take(40).collect();
        assert!(tags_match(&row, std::slice::from_ref(&visible)));
        assert!(tags_match(&row, &[visible.clone(), "operator-note".into()]));
        assert!(!tags_match(&row, &[visible.clone(), visible]));
        assert!(!tags_match(&row, &[format!("slug:{slug}")]));
    }
}

#[test]
fn recovery_window_is_fixed_to_original_persisted_attempt_time() {
    let mut row = metadata_checkout();
    row.created_at = "2025-01-02T03:04:05Z".parse().unwrap();
    assert_eq!(
        draft_recovery_query(&row).unwrap(),
        "created_at:>='2025-01-02T02:59:05+00:00' created_at:<='2025-01-02T03:09:05+00:00'"
    );
    row.reservation_expires_at = Some(Utc::now() + Duration::hours(1));
    assert_eq!(
        draft_recovery_window(&row).unwrap(),
        (
            "2025-01-02T02:59:05Z".parse().unwrap(),
            "2025-01-02T03:09:05Z".parse().unwrap()
        )
    );
}

#[tokio::test]
async fn metadata_lost_create_recovery_scans_all_pages_and_requires_one_exact_draft() {
    let row = metadata_checkout();
    let value = metadata_draft(&row);
    let mut foreign = value.clone();
    foreign["customAttributes"] =
        serde_json::to_value(correlation::identity_attributes(&metadata_checkout()).unwrap())
            .unwrap();
    let shop = Fixture::new(vec![
        Reply::json(json!({"data":{"draftOrders":{"nodes":[foreign],"pageInfo":{"hasNextPage":true,"endCursor":"next"}}}})),
        Reply::json(json!({"data":{"draftOrders":{"nodes":[value],"pageInfo":{"hasNextPage":false,"endCursor":null}}}})),
    ]).await;
    let shopify = client(&shop);
    let clients = CheckoutClients {
        shopify: &shopify,
        stripe: None,
        paypal: None,
    };
    assert_eq!(
        find_reserved_draft(&clients, &row).await.unwrap(),
        "gid://shopify/DraftOrder/1"
    );
    let requests = shop.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(
            request.2["variables"]["query"],
            draft_recovery_query(&row).unwrap()
        );
        assert_eq!(request.2["variables"]["sortKey"], "CREATED_AT");
    }
    assert_eq!(requests[1].2["variables"]["after"], "next");
}

#[tokio::test]
async fn metadata_lost_create_recovery_rejects_missing_duplicate_changed_or_unbounded_evidence() {
    let row = metadata_checkout();
    let value = metadata_draft(&row);
    let mut malformed = value.clone();
    malformed["customAttributes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"key":"checkout_id","value":row.id}));
    let mut missing_created = value.clone();
    missing_created.as_object_mut().unwrap().remove("createdAt");
    let mut outside = value.clone();
    outside["createdAt"] = json!(row.created_at + Duration::hours(1));
    let mut wrong_quantity = value.clone();
    wrong_quantity["lineItems"]["nodes"][0]["quantity"] = json!(1);
    let mut missing_reservation = value.clone();
    missing_reservation["reserveInventoryUntil"] = serde_json::Value::Null;
    for nodes in [
        vec![],
        vec![value.clone(), value.clone()],
        vec![malformed],
        vec![missing_created],
        vec![outside],
        vec![wrong_quantity],
        vec![missing_reservation],
    ] {
        let shop = Fixture::new(vec![Reply::json(json!({"data":{"draftOrders":{"nodes":nodes,"pageInfo":{"hasNextPage":false,"endCursor":null}}}}))]).await;
        let shopify = client(&shop);
        let clients = CheckoutClients {
            shopify: &shopify,
            stripe: None,
            paypal: None,
        };
        assert!(find_reserved_draft(&clients, &row).await.is_err());
        assert_eq!(shop.requests().len(), 1);
    }
}

#[tokio::test]
async fn metadata_recovery_never_accepts_partial_scan_or_scan_past_cap() {
    let row = metadata_checkout();
    let value = metadata_draft(&row);
    let mut replies = Vec::new();
    for page in 0..20 {
        replies.push(Reply::json(json!({"data":{"draftOrders":{"nodes":if page==0 {vec![value.clone()]} else {vec![]},"pageInfo":{"hasNextPage":true,"endCursor":format!("cursor-{page}")}}}})));
    }
    let shop = Fixture::new(replies).await;
    let shopify = client(&shop);
    let clients = CheckoutClients {
        shopify: &shopify,
        stripe: None,
        paypal: None,
    };
    assert!(find_reserved_draft(&clients, &row).await.is_err());
    assert_eq!(shop.requests().len(), 20);
    let shop=Fixture::new(vec![Reply::json(json!({"data":{"draftOrders":{"nodes":[value],"pageInfo":{"hasNextPage":true,"endCursor":null}}}}))]).await;
    let shopify = client(&shop);
    let clients = CheckoutClients {
        shopify: &shopify,
        stripe: None,
        paypal: None,
    };
    assert!(find_reserved_draft(&clients, &row).await.is_err());
}

#[tokio::test]
async fn metadata_recovery_rejects_second_match_or_failed_later_page_after_first_match() {
    let row = metadata_checkout();
    let value = metadata_draft(&row);
    let mut duplicate = value.clone();
    duplicate["id"] = json!("gid://shopify/DraftOrder/2");
    for later in [
        Reply::json(
            json!({"data":{"draftOrders":{"nodes":[duplicate],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}),
        ),
        Reply::status(500),
    ] {
        let shop = Fixture::new(vec![
            Reply::json(json!({"data":{"draftOrders":{"nodes":[value.clone()],"pageInfo":{"hasNextPage":true,"endCursor":"next"}}}})), later,
        ]).await;
        let shopify = client(&shop);
        let clients = CheckoutClients {
            shopify: &shopify,
            stripe: None,
            paypal: None,
        };
        assert!(find_reserved_draft(&clients, &row).await.is_err());
        assert!(shop.requests().len() >= 2);
        assert!(
            shop.requests()
                .iter()
                .all(|request| request.2["operationName"] == "DraftOrders")
        );
    }
}

#[test]
fn customer_fulfillment_notes_do_not_change_financial_purchase_evidence() {
    let mut purchase = PreparedPurchase {
        email: "buyer@example.test".into(),
        note: None,
        money: CheckoutMoney {
            minor: 2000,
            currency: "SGD".into(),
        },
        items: vec![PreparedItem {
            variant_id: Uuid::new_v4(),
            shopify_product_id: "gid://shopify/Product/1".into(),
            shopify_variant_id: "gid://shopify/ProductVariant/2".into(),
            quantity: 1,
            unit_price_minor: 2000,
        }],
        tags: vec!["example-convention".into()],
        slug: Some("example-convention".into()),
    };
    for version in [1, 2] {
        let original = purchase.evidence_hash(version).unwrap();
        purchase.note = Some("Mutable packing instructions".into());
        assert_eq!(purchase.evidence_hash(version).unwrap(), original);
        purchase.note = None;
    }
}

use super::*;
use chrono::Duration;
use domain::dto::checkout::CheckoutMoney;
use std::collections::BTreeMap;

pub(super) fn checkout() -> Checkout {
    let id = Uuid::new_v4();
    let now = Utc::now();
    let money = money();
    let lines = BTreeMap::from([("gid://shopify/ProductVariant/2".into(), 2)]);
    Checkout {
        id,
        completion_version: 1,
        correlation_version: 1,
        convention_slug: None,
        payment_amount_minor: None,
        payment_currency: None,
        shopify_amount_minor: None,
        shopify_currency: None,
        payment_recording_state: crate::models::checkout::PaymentRecordingState::NotStarted,
        guest_secret_hash: vec![17; 32],
        owner_expires_at: now + Duration::days(7),
        request_fingerprint: vec![0; 32],
        convention_id: Uuid::new_v4(),
        provider: "stripe".into(),
        provider_account_id: "acct_test".into(),
        provider_environment: "false".into(),
        evidence_hash: purchase::evidence_hash("private@example.test", &money, &lines)
            .expect("evidence"),
        required_tag_hashes: vec![Sha256::digest(format!("checkout:{id}").as_bytes()).to_vec()],
        success_url: Some(format!("https://example.test/return?checkout_id={id}")),
        cancel_url: Some(format!("https://example.test/cancel?checkout_id={id}")),
        gateway_request_fingerprint: None,
        payment_state: PaymentState::RequiresAction,
        draft_state: DraftState::Reserved,
        completion_state: CompletionState::NotStarted,
        draft_id: Some("gid://shopify/DraftOrder/1".into()),
        provider_resource_id: Some("cs_test".into()),
        order_id: None,
        reservation_expires_at: Some(now + Duration::hours(1)),
        created_at: now,
    }
}

pub(super) fn money() -> CheckoutMoney {
    CheckoutMoney {
        minor: 4000,
        currency: "SGD".into(),
    }
}

pub(super) fn draft(row: &Checkout) -> serde_json::Value {
    serde_json::json!({
        "id":"gid://shopify/DraftOrder/1", "ready":true, "status":"OPEN", "order":null,
        "tags":[format!("checkout:{}",row.id)], "email":"private@example.test", "paymentTerms":null,
        "totalPriceSet":{"presentmentMoney":{"amount":"40.00","currencyCode":"SGD"}},
        "reserveInventoryUntil":row.reservation_expires_at,
        "lineItems":{"nodes":[{"quantity":2,"variant":{"id":"gid://shopify/ProductVariant/2"}}],
            "pageInfo":{"hasNextPage":false,"endCursor":null}}
    })
}

#[test]
fn credentials_require_exact_secret_and_unexpired_owner() {
    let mut row = checkout();
    assert!(authorize(&row, &[17; 32]).is_ok());
    assert!(authorize(&row, &[18; 32]).is_err());
    assert!(authorize(&row, &[17; 31]).is_err());
    row.owner_expires_at = Utc::now() - Duration::seconds(1);
    assert!(authorize(&row, &[17; 32]).is_err());
}

#[test]
fn paid_expired_or_uncertain_purchase_never_exposes_payment_action() {
    let mut row = checkout();
    row.payment_state = PaymentState::Paid;
    row.reservation_expires_at = Some(Utc::now() - Duration::seconds(1));
    assert_eq!(
        response_status(&row, false),
        CheckoutStatus::ReconciliationRequired
    );
    row.completion_state = CompletionState::Uncertain;
    assert_eq!(
        response_status(&row, false),
        CheckoutStatus::ReconciliationRequired
    );
    assert_eq!(
        response_status(&checkout(), true),
        CheckoutStatus::Processing
    );
}

#[test]
fn purchase_evidence_rejects_changed_email_money_variants_quantities_and_required_tags() {
    let row = checkout();
    let base = draft(&row);
    let parse = |value| {
        serde_json::from_value::<crate::models::shopify::CompletionDraft>(value).expect("fixture")
    };
    assert!(purchase::validate_purchase(&row, &parse(base.clone())).is_ok());
    for field in [
        "quantity",
        "variant",
        "total",
        "currency",
        "email",
        "tag",
        "missing_line",
        "pagination",
    ] {
        let mut changed = base.clone();
        match field {
            "quantity" => changed["lineItems"]["nodes"][0]["quantity"] = 1.into(),
            "variant" => {
                changed["lineItems"]["nodes"][0]["variant"]["id"] =
                    "gid://shopify/ProductVariant/3".into();
            }
            "total" => changed["totalPriceSet"]["presentmentMoney"]["amount"] = "39.00".into(),
            "currency" => {
                changed["totalPriceSet"]["presentmentMoney"]["currencyCode"] = "USD".into();
            }
            "email" => changed["email"] = "other@example.test".into(),
            "tag" => changed["tags"] = serde_json::json!(["checkout:other"]),
            "missing_line" => changed["lineItems"]["nodes"] = serde_json::json!([]),
            "pagination" => changed["lineItems"]["pageInfo"]["hasNextPage"] = true.into(),
            _ => unreachable!(),
        }
        assert!(
            purchase::validate_purchase(&row, &parse(changed)).is_err(),
            "{field}"
        );
    }
    let mut added = base;
    added["tags"]
        .as_array_mut()
        .expect("tags")
        .push("unrelated".into());
    added["totalPriceSet"]["presentmentMoney"]["amount"] = "40.0000".into();
    assert!(purchase::validate_purchase(&row, &parse(added)).is_ok());
}

#[test]
fn evidence_canonicalization_ignores_line_order_but_commits_email_and_money() {
    let a = BTreeMap::from([("variant-b".into(), 2), ("variant-a".into(), 1)]);
    let b = BTreeMap::from([("variant-a".into(), 1), ("variant-b".into(), 2)]);
    assert_eq!(
        purchase::evidence_hash("a@example.test", &money(), &a).expect("hash"),
        purchase::evidence_hash("a@example.test", &money(), &b).expect("hash")
    );
    assert_ne!(
        purchase::evidence_hash("a@example.test", &money(), &a).expect("hash"),
        purchase::evidence_hash("b@example.test", &money(), &a).expect("hash")
    );
}

#[test]
fn initiation_fingerprint_commits_frozen_parameters_and_uses_seconds_precision() {
    use chrono::SubsecRound;
    let mut row = checkout();
    let original = provider::initiation_fingerprint(&row, &money()).expect("hash");
    row.reservation_expires_at = row.reservation_expires_at.map(|time| time.trunc_subsecs(6));
    assert_eq!(
        original,
        provider::initiation_fingerprint(&row, &money()).expect("hash")
    );
    row.success_url = Some("https://changed.test/return".into());
    assert_ne!(
        original,
        provider::initiation_fingerprint(&row, &money()).expect("hash")
    );
    row = checkout();
    let original = provider::initiation_fingerprint(&row, &money()).expect("hash");
    row.provider_environment = "true".into();
    assert_ne!(
        original,
        provider::initiation_fingerprint(&row, &money()).expect("hash")
    );
}

#[test]
fn paypal_uses_provider_whole_units_without_changing_canonical_money() {
    for (currency, minor, expected) in [
        ("SGD", 4000, "40.00"),
        ("HUF", 4000, "40"),
        ("TWD", 4000, "40"),
        ("JPY", 40, "40"),
    ] {
        let money = CheckoutMoney {
            minor,
            currency: currency.into(),
        };
        assert_eq!(
            provider::paypal_amount(&money).expect("provider money"),
            expected
        );
        assert_eq!(money.minor, minor);
    }
    assert!(
        provider::paypal_amount(&CheckoutMoney {
            minor: 4001,
            currency: "HUF".into()
        })
        .is_err()
    );
}

#[test]
fn recovered_draft_requires_ready_open_unlinked_purchase_without_terms() {
    let row = checkout();
    let base = draft(&row);
    let parse = |value| {
        serde_json::from_value::<crate::models::shopify::CompletionDraft>(value).expect("fixture")
    };
    assert!(purchase::validate_payable(&parse(base.clone())).is_ok());
    for field in ["ready", "status", "order", "paymentTerms"] {
        let mut changed = base.clone();
        changed[field] = match field {
            "ready" => false.into(),
            "status" => "COMPLETED".into(),
            "order" => serde_json::json!({"id":"gid://shopify/Order/1"}),
            "paymentTerms" => serde_json::json!({"id":"gid://shopify/PaymentTerms/1"}),
            _ => unreachable!(),
        };
        assert!(
            purchase::validate_payable(&parse(changed)).is_err(),
            "{field}"
        );
    }
}

#[test]
fn reservation_deadline_response_is_stable_after_database_precision_roundtrip() {
    use chrono::SubsecRound;
    let mut row = checkout();
    let deadline = chrono::DateTime::parse_from_rfc3339("2035-01-02T03:04:05.123456789Z")
        .expect("timestamp")
        .with_timezone(&Utc);
    row.reservation_expires_at = Some(deadline);
    let first = crate::transformers::checkout::response(
        &row,
        CheckoutStatus::Processing,
        money(),
        None,
        None,
    );
    row.reservation_expires_at = Some(deadline.trunc_subsecs(6));
    let recovered = crate::transformers::checkout::response(
        &row,
        CheckoutStatus::Processing,
        money(),
        None,
        None,
    );
    assert_eq!(
        first.reservation_expires_at,
        recovered.reservation_expires_at
    );
    assert_eq!(
        first.reservation_expires_at.as_deref(),
        Some("2035-01-02T03:04:05.123456+00:00")
    );
}

#[test]
fn uncertain_payment_requires_reconciliation_without_a_payment_action() {
    let mut row = checkout();
    row.payment_state = PaymentState::Uncertain;
    assert_eq!(
        response_status(&row, false),
        CheckoutStatus::ReconciliationRequired
    );
    assert_eq!(
        response_status(&row, true),
        CheckoutStatus::ReconciliationRequired
    );
}

#[tokio::test]
async fn internal_recovery_uses_owned_row_without_guest_auth_or_another_claim() {
    let shop = crate::client::tests::Fixture::new(vec![]).await;
    let shopify = crate::services::common::client(&shop);
    let clients = CheckoutClients {
        shopify: &shopify,
        stripe: None,
        paypal: None,
    };
    let settings = config::Config::builder()
        .add_source(config::File::from_str(
            include_str!("../../../application.toml"),
            config::FileFormat::Toml,
        ))
        .add_source(config::File::from_str(
            include_str!("../../../application.qa.toml"),
            config::FileFormat::Toml,
        ))
        .build()
        .expect("QA non-sensitive configuration")
        .get::<CheckoutSettings>("checkout")
        .expect("checkout settings");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@127.0.0.1:1/fixture")
        .expect("lazy pool");
    pool.close().await;
    let mut row = checkout();
    row.owner_expires_at = Utc::now() - Duration::days(1);
    row.draft_state = DraftState::NotStarted;
    row.draft_id = None;
    let original_owner_expiry = row.owner_expires_at;
    assert_eq!(
        recover_claimed(&pool, &settings, &clients, &mut row, Uuid::new_v4())
            .await
            .expect("safe outcome"),
        RecoveryOutcome::Manual(crate::models::checkout::RecoveryCode::MissingPurchase)
    );
    assert_eq!(row.owner_expires_at, original_owner_expiry);
    assert_eq!(shop.requests().len(), 0);
}

#[tokio::test]
async fn completed_v2_receipt_rejects_refunded_or_wrong_named_gateway_truth() {
    use crate::client::tests::{Fixture, Reply};
    use serde_json::json;
    let mut row = checkout();
    row.completion_version = 2;
    row.convention_slug = Some("qa-convention".into());
    row.payment_state = PaymentState::Paid;
    row.completion_state = CompletionState::Completed;
    row.order_id = Some("gid://shopify/Order/2".into());
    row.evidence_hash = Sha256::digest(
        serde_json::to_vec(&(
            "private@example.test",
            money(),
            BTreeMap::from([("gid://shopify/ProductVariant/2", 2)]),
            "qa-convention",
        ))
        .expect("fixture"),
    )
    .to_vec();
    let mut original = draft(&row);
    original["customAttributes"] = json!([{"key":"convention_slug","value":"qa-convention"}]);
    original["status"] = json!("COMPLETED");
    original["order"] = json!({"id":"gid://shopify/Order/2"});
    for financial_status in ["REFUNDED", "PAID"] {
        let shop=Fixture::new(vec![Reply::json(json!({"data":{"draftOrder":original}})),Reply::json(json!({"data":{"order":{
            "id":"gid://shopify/Order/2","name":"#2","displayFinancialStatus":financial_status,"cancelledAt":null,
            "tags":[format!("checkout:{}",row.id)],"customAttributes":[{"key":"convention_slug","value":"qa-convention"}],
            "totalPriceSet":{"presentmentMoney":{"amount":"40.00","currencyCode":"SGD"}},
            "totalOutstandingSet":{"presentmentMoney":{"amount":"0.00","currencyCode":"SGD"}},
            "transactions":[{"id":"gid://shopify/OrderTransaction/3","gateway":"manual","status":"SUCCESS","kind":"SALE","amountSet":{"presentmentMoney":{"amount":"40.00","currencyCode":"SGD"}}}]
        }}}))]).await;
        let shopify = crate::services::common::client(&shop);
        let clients = CheckoutClients {
            shopify: &shopify,
            stripe: None,
            paypal: None,
        };
        assert!(
            hydrate_response(&clients, &row, false).await.is_err(),
            "accepted {financial_status}"
        );
    }
}

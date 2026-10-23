use super::*;
use crate::models::checkout::{CheckoutProviderContext, CheckoutSnapshot};
use chrono::Duration;
use domain::dto::checkout::CheckoutMoney;
use sqlx::types::Json;

fn checkout() -> Checkout {
    let id = Uuid::new_v4();
    let now = Utc::now();
    Checkout {
        id,
        guest_secret_hash: vec![17; 32],
        owner_expires_at: now + Duration::days(7),
        request_fingerprint: vec![0; 32],
        provider: "stripe".into(),
        snapshot: Json(CheckoutSnapshot {
            convention_id: Uuid::new_v4(),
            convention_slug: "test".into(),
            convention_name: "Test".into(),
            email: "private@example.test".into(),
            money: CheckoutMoney {
                minor: 4000,
                currency: "SGD".into(),
            },
            items: vec![],
        }),
        provider_context: Json(CheckoutProviderContext {
            account_id: "acct_test".into(),
            environment: "false".into(),
            correlation_id: id.to_string(),
        }),
        payment_state: PaymentState::RequiresAction,
        draft_state: DraftState::Reserved,
        completion_state: CompletionState::NotStarted,
        draft_id: Some("gid://shopify/DraftOrder/1".into()),
        provider_resource_id: Some("cs_test".into()),
        payment_reference: None,
        order_id: None,
        order_name: None,
        redirect_url: Some("https://checkout.stripe.com/test".into()),
        reservation_expires_at: Some(now + Duration::hours(1)),
        create_request_id: id.to_string(),
        capture_request_id: id.to_string(),
        created_at: now,
    }
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
fn paid_expired_purchase_requires_reconciliation_and_never_redirects() {
    let mut row = checkout();
    row.payment_state = PaymentState::Paid;
    row.reservation_expires_at = Some(Utc::now() - Duration::seconds(1));
    let result = response(&row, false);
    assert_eq!(result.status, CheckoutStatus::ReconciliationRequired);
    assert!(result.next_action.is_none());
    assert!(result.order.is_none());
}

#[test]
fn uncertain_completion_never_exposes_success_or_payment_redirect() {
    let mut row = checkout();
    row.completion_state = CompletionState::Uncertain;
    assert_eq!(
        response(&row, false).status,
        CheckoutStatus::ReconciliationRequired
    );
    assert!(response(&row, false).next_action.is_none());
}

#[test]
fn busy_checkout_never_reissues_a_browser_payment_action() {
    assert_eq!(
        response(&checkout(), true).status,
        CheckoutStatus::Processing
    );
    assert!(response(&checkout(), true).next_action.is_none());
}

#[test]
fn reservation_requires_exact_lines_total_owner_and_hold() {
    use crate::models::checkout::CheckoutItemSnapshot;
    let mut row = checkout();
    row.snapshot.items.push(CheckoutItemSnapshot {
        variant_id: Uuid::new_v4(),
        shopify_product_id: "gid://shopify/Product/1".into(),
        shopify_variant_id: "gid://shopify/ProductVariant/2".into(),
        product_title: None,
        variant_title: None,
        sku: None,
        quantity: 2,
        unit_price_minor: 2000,
        total_minor: 4000,
    });
    let deadline = row.reservation_expires_at.expect("fixture reservation");
    let base = serde_json::json!({
        "id":"gid://shopify/DraftOrder/1", "ready":true, "status":"OPEN", "order":null,
        "tags":purchase::convention_tags(&row), "email":"private@example.test", "paymentTerms":null,
        "totalPriceSet":{"presentmentMoney":{"amount":"40.00","currencyCode":"SGD"}},
        "reserveInventoryUntil":deadline,
        "lineItems":{"nodes":[{"quantity":2,"variant":{"id":"gid://shopify/ProductVariant/2"}}],
            "pageInfo":{"hasNextPage":false,"endCursor":null}}
    });
    let parse = |value| {
        serde_json::from_value::<crate::models::shopify::CompletionDraft>(value)
            .expect("fixture draft")
    };
    assert!(purchase::validate_draft(&row, &parse(base.clone()), deadline).is_ok());
    for field in [
        "quantity",
        "total",
        "email",
        "hold",
        "missing_line",
        "pagination",
    ] {
        let mut changed = base.clone();
        match field {
            "quantity" => changed["lineItems"]["nodes"][0]["quantity"] = 1.into(),
            "total" => changed["totalPriceSet"]["presentmentMoney"]["amount"] = "39.00".into(),
            "email" => changed["email"] = "other@example.test".into(),
            "hold" => changed["reserveInventoryUntil"] = serde_json::Value::Null,
            "missing_line" => changed["lineItems"]["nodes"] = serde_json::json!([]),
            "pagination" => changed["lineItems"]["pageInfo"]["hasNextPage"] = true.into(),
            _ => unreachable!(),
        }
        assert!(
            purchase::validate_draft(&row, &parse(changed), deadline).is_err(),
            "{field}"
        );
    }
}

#[test]
fn paypal_serializes_its_own_whole_currency_units_without_changing_purchase_money() {
    for (currency, minor, expected) in [
        ("SGD", 4000, "40.00"),
        ("HUF", 4000, "40"),
        ("TWD", 4000, "40"),
        ("JPY", 40, "40"),
    ] {
        let mut row = checkout();
        row.snapshot.money.currency = currency.into();
        row.snapshot.money.minor = minor;
        assert_eq!(
            provider::paypal_amount(&row).expect("provider money"),
            expected
        );
        assert_eq!(row.snapshot.money.minor, minor);
    }
    let mut row = checkout();
    row.snapshot.money.currency = "HUF".into();
    row.snapshot.money.minor = 4001;
    assert!(provider::paypal_amount(&row).is_err());
}

#[test]
fn reservation_deadline_response_is_stable_after_database_precision_roundtrip() {
    use chrono::SubsecRound;
    let mut row = checkout();
    let deadline = chrono::DateTime::parse_from_rfc3339("2035-01-02T03:04:05.123456789Z")
        .expect("valid nanosecond timestamp")
        .with_timezone(&Utc);
    row.reservation_expires_at = Some(deadline);
    let first = response(&row, false);
    row.reservation_expires_at = Some(deadline.trunc_subsecs(6));
    let recovered = response(&row, false);
    assert_eq!(
        first.reservation_expires_at,
        recovered.reservation_expires_at
    );
}

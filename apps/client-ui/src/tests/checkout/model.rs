use super::*;
use domain::dto::{
    ConventionQuoteLine,
    checkout::{CheckoutMoney, CheckoutOrder},
};

fn cart(price: &str) -> Cart {
    let mut cart = Cart::new(
        "c94f1e30-1954-4e39-9e1c-9c66c162ce22".into(),
        "f79b7e64-8e36-4aca-9e21-ecb7cb513700".into(),
    );
    cart.add(&crate::tests::variant(), 2)
        .expect("fixture selection");
    cart.items[0].accepted_unit_price = price.into();
    cart
}
fn quoted(currency: &str, unit: &str, total: &str, zero: &str) -> ConventionQuote {
    ConventionQuote {
        currency_code: currency.into(),
        items: vec![ConventionQuoteLine {
            variant_id: crate::tests::variant().id,
            quantity: 2,
            unit_price: unit.into(),
            line_total: total.into(),
        }],
        subtotal: total.into(),
        shipping: zero.into(),
        total: total.into(),
    }
}
fn attempt() -> Attempt {
    Attempt {
        version: 1,
        operation_id: "f79b7e64-8e36-4aca-9e21-ecb7cb513700".into(),
        secret: "ab".repeat(32),
        slug: "convention".into(),
        provider: CheckoutProvider::Stripe,
        snapshot: cart("12.30"),
        state: "submitted".into(),
        currency: "USD".into(),
        total: "24.60".into(),
    }
}
fn response() -> CheckoutResponse {
    CheckoutResponse {
        checkout_id: attempt().operation_id,
        status: CheckoutStatus::RequiresAction,
        money: CheckoutMoney {
            minor: 2460,
            currency: "USD".into(),
        },
        reservation_expires_at: None,
        next_action: Some(CheckoutNextAction::Redirect {
            url: "https://checkout.stripe.com/c/pay/session".into(),
        }),
        order: None,
    }
}

#[test]
fn authoritative_quote_preserves_exact_zero_two_and_three_decimal_minor_units() {
    for (currency, unit, total, zero, expected) in [
        ("JPY", "12", "24", "0", 12),
        ("SGD", "12.30", "24.60", "0.00", 1230),
        ("BHD", "0.123", "0.246", "0.000", 123),
        (
            "USD",
            "90071992547409.93",
            "180143985094819.86",
            "0.00",
            9_007_199_254_740_993,
        ),
    ] {
        let result = request(
            &cart(unit),
            &quoted(currency, unit, total, zero),
            "buyer@example.test",
            CheckoutProvider::Paypal,
        )
        .expect("accepted current quote");
        assert_eq!(result.items[0].expected_unit_price, expected);
        assert_eq!(result.items[0].variant_id, crate::tests::variant().id);
        assert_eq!(result.expected_currency, currency);
        assert_eq!(result.email, "buyer@example.test");
    }
}

#[test]
fn stale_prices_mismatched_lines_shipping_invalid_email_and_overflow_block_creation() {
    let valid = quoted("USD", "12.30", "24.60", "0.00");
    assert!(request(&cart("12.30"), &valid, "bad", CheckoutProvider::Stripe).is_err());
    assert!(
        request(
            &cart("12.00"),
            &valid,
            "buyer@example.test",
            CheckoutProvider::Stripe
        )
        .is_err()
    );
    let mut altered = valid.clone();
    altered.items[0].quantity = 1;
    assert!(
        request(
            &cart("12.30"),
            &altered,
            "buyer@example.test",
            CheckoutProvider::Stripe
        )
        .is_err()
    );
    altered = valid.clone();
    altered.shipping = "1.00".into();
    assert!(
        request(
            &cart("12.30"),
            &altered,
            "buyer@example.test",
            CheckoutProvider::Stripe
        )
        .is_err()
    );
    altered = valid.clone();
    altered.total = "1.00".into();
    assert!(
        request(
            &cart("12.30"),
            &altered,
            "buyer@example.test",
            CheckoutProvider::Stripe
        )
        .is_err()
    );
    assert!(minor("92233720368547758.08", "USD").is_none());
    for invalid in ["NaN", "-1", "1.", ".5", "1.2345", "1e3"] {
        assert!(minor(invalid, "USD").is_none());
    }
}

#[test]
fn empty_cart_cannot_create_payment_even_with_an_empty_quote() {
    let mut empty = cart("12.30");
    empty.items.clear();
    assert!(
        request(
            &empty,
            &quoted("USD", "12.30", "24.60", "0.00"),
            "buyer@example.test",
            CheckoutProvider::Stripe
        )
        .is_err()
    );
}

#[test]
fn only_bound_verified_completed_order_can_show_success_and_preserves_shopify_display_name() {
    let attempt = attempt();
    let mut result = response();
    result.status = CheckoutStatus::Completed;
    assert!(checked_response(&attempt, &result).is_err());
    result.order = Some(CheckoutOrder {
        id: "gid://shopify/Order/12".into(),
        name: "PREORDER-1234".into(),
    });
    assert!(checked_response(&attempt, &result).is_ok());
    result.money.minor += 1;
    assert!(checked_response(&attempt, &result).is_err());
    result.money.minor -= 1;
    result.checkout_id = "another".into();
    assert!(checked_response(&attempt, &result).is_err());
}

#[test]
fn provider_redirects_reject_lookalike_hosts_credentials_ports_and_nonactionable_states() {
    let mut result = response();
    assert!(
        redirect(&result, CheckoutProvider::Stripe, None)
            .expect("valid redirect")
            .is_some()
    );
    for url in [
        "http://checkout.stripe.com/pay",
        "https://checkout.stripe.com.evil.test/pay",
        "https://evil.test/?next=https://checkout.stripe.com",
        "https://user@checkout.stripe.com/pay",
        "https://checkout.stripe.com:9999/pay",
        "javascript:alert(1)",
        "https://www.paypal.com/pay",
        "https://checkout.stripe.com.evil.test/c/pay/cs_test_example#provider-data",
        "https://user:password@checkout.stripe.com/c/pay/cs_test_example#provider-data",
        "https://checkout.stripe.com:9999/c/pay/cs_test_example#provider-data",
    ] {
        result.next_action = Some(CheckoutNextAction::Redirect { url: url.into() });
        assert!(
            redirect(&result, CheckoutProvider::Stripe, None).is_err(),
            "{url}"
        );
    }
    result.next_action = Some(CheckoutNextAction::Redirect {
        url: "https://localhost:18047/provider/stripe".into(),
    });
    assert!(redirect(&result, CheckoutProvider::Stripe, None).is_err());
    assert!(
        redirect(
            &result,
            CheckoutProvider::Stripe,
            Some("https://localhost:18047")
        )
        .is_ok()
    );
    result.status = CheckoutStatus::Processing;
    assert_eq!(
        redirect(&result, CheckoutProvider::Stripe, None).expect("valid checkout fixture"),
        None
    );
}

#[test]
fn stripe_hosted_checkout_preserves_its_provider_fragment() {
    let url = "https://checkout.stripe.com/c/pay/cs_test_example#fidkdWxOYHwnPyd1blpxYHZxWjA0test%2Fencoded";
    let mut result = response();
    result.next_action = Some(CheckoutNextAction::Redirect { url: url.into() });
    assert_eq!(
        redirect(&result, CheckoutProvider::Stripe, None).expect("Stripe hosted checkout"),
        Some(url.into())
    );
}

#[test]
fn fragments_remain_unsupported_for_paypal_and_controlled_local_redirects() {
    let mut result = response();
    for url in [
        "https://www.paypal.com/checkoutnow?token=order#fragment",
        "https://www.sandbox.paypal.com/checkoutnow?token=order#fragment",
    ] {
        result.next_action = Some(CheckoutNextAction::Redirect { url: url.into() });
        assert!(redirect(&result, CheckoutProvider::Paypal, None).is_err());
    }
    result.next_action = Some(CheckoutNextAction::Redirect {
        url: "https://localhost:18047/provider/stripe#fragment".into(),
    });
    assert!(
        redirect(
            &result,
            CheckoutProvider::Stripe,
            Some("https://localhost:18047")
        )
        .is_err()
    );
}

#[test]
fn paypal_production_and_sandbox_hosts_are_actionable_only_for_paypal() {
    for url in [
        "https://www.paypal.com/checkoutnow?token=order",
        "https://www.sandbox.paypal.com/checkoutnow?token=order",
    ] {
        let mut result = response();
        result.next_action = Some(CheckoutNextAction::Redirect { url: url.into() });
        assert_eq!(
            redirect(&result, CheckoutProvider::Paypal, None).expect("valid checkout fixture"),
            Some(url.into())
        );
    }
}

#[test]
fn recovered_create_rejects_any_changed_intent_or_cart_snapshot() {
    let attempt = attempt();
    let mut saved = request(
        &attempt.snapshot,
        &quoted("USD", "12.30", "24.60", "0.00"),
        "buyer@example.test",
        attempt.provider,
    )
    .expect("valid checkout fixture");
    saved.operation_id = attempt.operation_id.clone();
    assert!(saved_request_matches(&attempt, &saved));
    saved.items[0].quantity = 3;
    assert!(!saved_request_matches(&attempt, &saved));
    saved.items[0].quantity = 2;
    saved.provider = CheckoutProvider::Paypal;
    assert!(!saved_request_matches(&attempt, &saved));
    saved.provider = attempt.provider;
    saved.operation_id = "another".into();
    assert!(!saved_request_matches(&attempt, &saved));
}

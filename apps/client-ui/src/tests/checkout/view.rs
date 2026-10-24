use super::*;
use domain::dto::checkout::{CheckoutMoney, CheckoutOrder};

fn render(status: CheckoutStatus, order: Option<CheckoutOrder>) -> String {
    #[derive(Clone)]
    struct Fixture(CheckoutResponse);
    fn app() -> Element {
        let response = use_context::<Fixture>().0;
        rsx! { StatusContent {response,provider:CheckoutProvider::Stripe} }
    }
    let response = CheckoutResponse {
        checkout_id: "checkout".into(),
        status,
        money: CheckoutMoney {
            minor: 2460,
            currency: "USD".into(),
        },
        reservation_expires_at: None,
        next_action: None,
        order,
    };
    let mut dom = VirtualDom::new(app).with_root_context(Fixture(response));
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[test]
fn completed_view_renders_real_shopify_reference_and_confirmation_pickup_instructions() {
    let html = render(
        CheckoutStatus::Completed,
        Some(CheckoutOrder {
            id: "gid://shopify/Order/12".into(),
            name: "#1234".into(),
        }),
    );
    assert!(html.contains("Congratulations!"));
    assert!(html.contains("#1234"));
    assert!(html.contains("Check your email for your order confirmation. Show that email at the convention to confirm your preorder."));
    assert!(!html.contains("gid://shopify"));
}

#[test]
fn pending_and_manual_recovery_do_not_offer_another_payment_or_claim_success() {
    for status in [
        CheckoutStatus::Processing,
        CheckoutStatus::ReconciliationRequired,
    ] {
        let html = render(status, None);
        assert!(html.contains("do not pay again"));
        assert!(!html.contains("Congratulations"));
        assert!(!html.contains("Pay with"));
    }
}

#[test]
fn unpaid_and_requires_action_render_distinct_payment_guidance() {
    assert!(
        render(CheckoutStatus::Unpaid, None).contains("gateway confirmed this payment is unpaid")
    );
    let actionable = render(CheckoutStatus::RequiresAction, None);
    assert!(actionable.contains("Continue payment"));
    assert!(!actionable.contains("Congratulations"));
}

#[test]
fn payment_marks_render_both_gateway_and_wallet_symbols_without_stripe_button_copy() {
    fn app() -> Element {
        rsx! {PaymentMarks{provider:CheckoutProvider::Paypal} PaymentMarks{provider:CheckoutProvider::Stripe}}
    }
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("checkout-paypal-mark"));
    assert!(html.contains("checkout-wallet-marks"));
    assert!(!html.contains("Stripe"));
}

#[cfg(not(target_arch = "wasm32"))]
mod mounted {
    use super::*;
    use crate::{
        api::tests::responses,
        pages::tests::{Screen, TestDocument, click, input, mounted, until},
    };
    use serde_json::{Value, json};

    fn cart() -> Cart {
        let mut cart = Cart::new(
            "c94f1e30-1954-4e39-9e1c-9c66c162ce22".into(),
            "f79b7e64-8e36-4aca-9e21-ecb7cb513700".into(),
        );
        cart.add(&crate::tests::variant(), 2)
            .expect("valid checkout fixture");
        cart
    }
    fn quote() -> ConventionQuote {
        ConventionQuote {
            currency_code: "USD".into(),
            items: vec![domain::dto::ConventionQuoteLine {
                variant_id: crate::tests::variant().id,
                quantity: 2,
                unit_price: "12.30".into(),
                line_total: "24.60".into(),
            }],
            subtotal: "24.60".into(),
            shipping: "0.00".into(),
            total: "24.60".into(),
        }
    }
    fn attempt(provider: CheckoutProvider) -> Attempt {
        Attempt {
            version: 1,
            operation_id: "a2913eb5-722d-41a3-a521-632e7fa8cc9a".into(),
            secret: "ab".repeat(32),
            slug: "fair-a".into(),
            provider,
            snapshot: cart(),
            state: "submitted".into(),
            currency: "USD".into(),
            total: "24.60".into(),
        }
    }
    fn boot(attempt: &Attempt, fresh: bool) -> Value {
        let mut request = model::request(&cart(), &quote(), "buyer@example.test", attempt.provider)
            .expect("valid checkout fixture");
        request.operation_id = attempt.operation_id.clone();
        json!({"__eval_messages":[{"ok":true,"value":{"attempt":attempt,"payload":serde_json::to_string(&request).expect("valid checkout fixture"),"fresh":fresh}},{"ok":true,"value":false}]})
    }
    fn response(attempt: &Attempt, status: CheckoutStatus, url: Option<&str>) -> String {
        serde_json::to_string(&CheckoutResponse {
            checkout_id: attempt.operation_id.clone(),
            status,
            money: CheckoutMoney {
                minor: 2460,
                currency: "USD".into(),
            },
            reservation_expires_at: None,
            next_action: url
                .map(|url| domain::dto::checkout::CheckoutNextAction::Redirect { url: url.into() }),
            order: None,
        })
        .expect("valid checkout fixture")
    }

    #[tokio::test]
    async fn mounted_buttons_require_email_current_quote_and_nonempty_cart() {
        for (snapshot, quote) in [
            (cart(), Some(quote())),
            (cart(), None),
            (
                {
                    let mut c = cart();
                    c.items.clear();
                    c
                },
                Some(quote()),
            ),
        ] {
            responses(&[], async {
                let (mut dom, _) = mounted(
                    Screen::Checkout(snapshot.clone(), quote.clone()),
                    snapshot.clone(),
                    TestDocument::new(vec![json!({"ok":true,"value":null})]),
                );
                let html = until(&mut dom, "Pay with PayPal").await;
                assert_eq!(html.matches("checkout-payment-button").count(), 2);
                input(&mut dom, "checkout-email", "input", "invalid");
                let html = dioxus_ssr::render(&dom);
                assert!(html.contains("Enter a valid email"));
                click(&mut dom, "id", "checkout-paypal");
                assert!(!dioxus_ssr::render(&dom).contains("Preparing your secure payment"));
            })
            .await;
        }
    }

    #[tokio::test]
    async fn both_providers_create_once_with_guest_bearer_exact_quote_and_safe_hosted_redirect() {
        for (provider, url) in [
            (
                CheckoutProvider::Paypal,
                "https://www.sandbox.paypal.com/checkoutnow?token=order",
            ),
            (
                CheckoutProvider::Stripe,
                "https://checkout.stripe.com/c/pay/session",
            ),
        ] {
            let attempt = attempt(provider);
            let body = response(&attempt, CheckoutStatus::RequiresAction, Some(url));
            let doc = TestDocument::new(vec![
                json!({"ok":true,"value":null}),
                boot(&attempt, true),
                json!("http://localhost:8085/fair-a/cart/cart"),
                json!(true),
            ]);
            let inspect = doc.clone();
            let ((), requests) = responses(&[(200, &body)], async {
                let (mut dom, _) = mounted(Screen::Checkout(cart(), Some(quote())), cart(), doc);
                until(&mut dom, "Pay with PayPal").await;
                input(&mut dom, "checkout-email", "input", "buyer@example.test");
                let label = match provider {
                    CheckoutProvider::Paypal => "checkout-paypal",
                    CheckoutProvider::Stripe => "checkout-card",
                };
                click(&mut dom, "id", label);
                click(&mut dom, "id", label);
                let html = until(&mut dom, "Check existing payment").await;
                assert!(!html.contains("Congratulations"));
            })
            .await;
            assert_eq!(
                requests.len(),
                1,
                "one create even after repeated activation"
            );
            assert!(requests[0].starts_with("POST /v1/order/checkout "));
            assert!(
                requests[0]
                    .to_ascii_lowercase()
                    .contains(&format!("authorization: bearer {}", attempt.secret))
            );
            let sent: Value = serde_json::from_str(
                requests[0]
                    .split("\r\n\r\n")
                    .nth(1)
                    .expect("valid checkout fixture"),
            )
            .expect("valid checkout fixture");
            assert_eq!(sent["email"], "buyer@example.test");
            assert_eq!(sent["items"][0]["expected_unit_price"], 1230);
            assert_eq!(
                sent["provider"],
                serde_json::to_value(provider).expect("valid checkout fixture")
            );
            let scripts = inspect.scripts.borrow();
            assert!(
                scripts
                    .iter()
                    .any(|script| script.contains(&format!("window.location.assign(\"{url}\")")))
            );
            assert!(
                !requests
                    .iter()
                    .any(|request| request.to_ascii_lowercase().contains("cookie:"))
            );
        }
    }

    #[tokio::test]
    async fn return_with_paypal_query_parameters_verifies_without_catalogue_and_never_claims_http_success()
     {
        let attempt = attempt(CheckoutProvider::Paypal);
        let body = response(&attempt, CheckoutStatus::Processing, None);
        let doc = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            boot(&attempt, false),
            json!(false),
        ]);
        let ((), requests) = responses(&[(200, &body)], async {
            let path = format!(
                "/fair-a/checkout/return?checkout_id={}&token=untrusted&PayerID=untrusted",
                attempt.operation_id
            );
            let (mut dom, _) = mounted(Screen::Route(path), cart(), doc);
            let html = until(&mut dom, "Your payment is being checked").await;
            assert!(!html.contains("Congratulations"));
            assert!(html.contains("do not pay again"));
        })
        .await;
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains("/verify "));
        assert!(!requests[0].contains("untrusted"));
    }

    #[tokio::test]
    async fn missing_saved_credential_and_verify_failure_preserve_recovery_guidance() {
        responses(&[], async {
            let (mut dom, _) = mounted(
                Screen::Route(
                    "/fair-a/checkout/cancel?checkout_id=a2913eb5-722d-41a3-a521-632e7fa8cc9a"
                        .into(),
                ),
                cart(),
                TestDocument::new(vec![json!({"ok":true,"value":null})]),
            );
            let html = until(&mut dom, "This browser has no recovery information").await;
            assert!(!html.contains("Congratulations"));
        })
        .await;
        let attempt = attempt(CheckoutProvider::Paypal);
        responses(
            &[(401, r#"{"error":"Invalid checkout credential"}"#)],
            async {
                let mut expired_boot = boot(&attempt, false);
                expired_boot["__eval_messages"][0]["value"]["payload"] = Value::Null;
                let doc = TestDocument::new(vec![json!({"ok":true,"value":attempt}), expired_boot]);
                let (mut dom, _) = mounted(
                    Screen::Route(format!(
                        "/fair-a/checkout/cancel?checkout_id={}",
                        attempt.operation_id
                    )),
                    cart(),
                    doc,
                );
                let html = until(&mut dom, "can no longer verify this checkout").await;
                assert!(html.contains("does not confirm cancellation"));
                assert!(!html.contains("Pay with"));
            },
        )
        .await;
    }
    #[tokio::test]
    async fn unknown_checkout_after_lost_initial_request_replays_the_identical_saved_create() {
        let attempt = attempt(CheckoutProvider::Stripe);
        let body = response(&attempt, CheckoutStatus::Processing, None);
        let original = boot(&attempt, false);
        let expected_payload = original["__eval_messages"][0]["value"]["payload"]
            .as_str()
            .expect("valid checkout fixture")
            .to_owned();
        let doc = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            original,
            json!(false),
        ]);
        let ((), requests) = responses(
            &[
                (401, r#"{"error":"Invalid checkout credential"}"#),
                (200, &body),
            ],
            async {
                let (mut dom, _) = mounted(
                    Screen::Route(format!(
                        "/fair-a/checkout/return?checkout_id={}",
                        attempt.operation_id
                    )),
                    cart(),
                    doc,
                );
                let html = until(&mut dom, "Your payment is being checked").await;
                assert!(!html.contains("Congratulations"));
            },
        )
        .await;
        assert_eq!(requests.len(), 2);
        assert!(
            requests[0].starts_with(&format!("POST /v1/order/{}/verify ", attempt.operation_id))
        );
        assert!(requests[1].starts_with("POST /v1/order/checkout "));
        assert_eq!(
            requests[1]
                .split("\r\n\r\n")
                .nth(1)
                .expect("valid checkout fixture"),
            expected_payload
        );
        for request in requests {
            assert!(request.contains(&attempt.secret));
        }
    }
    #[tokio::test]
    async fn racing_tab_reviews_existing_provider_attempt_without_automatic_old_purchase_redirect()
    {
        let attempt = attempt(CheckoutProvider::Stripe);
        let body = response(
            &attempt,
            CheckoutStatus::RequiresAction,
            Some("https://checkout.stripe.com/c/pay/original"),
        );
        let doc = TestDocument::new(vec![
            json!({"ok":true,"value":null}),
            boot(&attempt, false),
            json!(true),
        ]);
        let inspected = doc.clone();
        let ((), requests) = responses(&[(200, &body)], async {
            let (mut dom, _) = mounted(Screen::Checkout(cart(), Some(quote())), cart(), doc);
            until(&mut dom, "Pay with PayPal").await;
            input(&mut dom, "checkout-email", "input", "buyer@example.test");
            click(&mut dom, "id", "checkout-paypal");
            until(&mut dom, "Check existing payment").await;
        })
        .await;
        assert_eq!(requests.len(), 1);
        assert!(requests.iter().all(|request| request.contains("/verify ")));
        let scripts = inspected.scripts.borrow();
        assert!(scripts.iter().all(|script| {
            !script.contains("window.location.assign(\"https://checkout.stripe.com")
        }));
        assert!(
            scripts
                .iter()
                .any(|script| script.contains("window.location.assign(\"/fair-a/checkout/return"))
        );
    }

    #[tokio::test]
    async fn malformed_return_links_never_read_browser_credentials_or_contact_the_api() {
        for query in [
            "token=provider-approval",
            "checkout_id=not-a-uuid",
            "checkout_id=a2913eb5-722d-41a3-a521-632e7fa8cc9a&checkout_id=a2913eb5-722d-41a3-a521-632e7fa8cc9a",
        ] {
            let doc = TestDocument::new(vec![]);
            let inspected = doc.clone();
            let ((), requests) = responses(&[], async {
                let (mut dom, _) = mounted(
                    Screen::Route(format!("/fair-a/checkout/return?{query}")),
                    cart(),
                    doc,
                );
                let html = until(&mut dom, "This checkout link is invalid").await;
                assert!(!html.contains("Congratulations"));
            })
            .await;
            assert_eq!(requests, Vec::<String>::new());
            assert!(inspected.scripts.borrow().is_empty());
        }
    }

    #[tokio::test]
    async fn explicit_status_retry_rechecks_the_same_payment_after_a_temporary_failure() {
        let attempt = attempt(CheckoutProvider::Paypal);
        let body = response(&attempt, CheckoutStatus::Unpaid, None);
        let doc = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            boot(&attempt, false),
            boot(&attempt, false),
        ]);
        let inspected = doc.clone();
        let ((), requests) = responses(
            &[(503, r#"{"error":"Provider unavailable"}"#), (200, &body)],
            async {
                let (mut dom, _) = mounted(
                    Screen::Route(format!(
                        "/fair-a/checkout/return?checkout_id={}",
                        attempt.operation_id
                    )),
                    cart(),
                    doc,
                );
                let html = until(&mut dom, "We could not confirm this payment yet").await;
                assert!(!html.contains("Congratulations"));
                click(&mut dom, "__text", "Check status");
                let html = until(&mut dom, "Payment was not completed").await;
                assert!(html.contains("Return to cart"));
                assert!(!html.contains("We could not confirm this payment yet"));
            },
        )
        .await;
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| {
            request.starts_with(&format!("POST /v1/order/{}/verify ", attempt.operation_id))
                && request.contains(&attempt.secret)
        }));
        assert_eq!(
            *inspected.sent.borrow(),
            vec![json!({"status":null}), json!({"status":"unpaid"})]
        );
    }

    #[tokio::test]
    async fn verified_completion_invalidates_only_the_matching_cached_cart() {
        for newer_cart in [false, true] {
            let attempt = attempt(CheckoutProvider::Stripe);
            let mut body: Value =
                serde_json::from_str(&response(&attempt, CheckoutStatus::Completed, None))
                    .expect("checkout response fixture");
            body["order"] = json!({"id":"gid://shopify/Order/1234","name":"#CONVENTION-1234"});
            let body = body.to_string();
            let doc = TestDocument::new(vec![
                json!({"ok":true,"value":attempt}),
                boot(&attempt, false),
            ]);
            let inspected = doc.clone();
            let ((), requests) = responses(&[(200, &body)], async {
                let (mut dom, _) = mounted(
                    Screen::Route(format!(
                        "/fair-a/checkout/return?checkout_id={}",
                        attempt.operation_id
                    )),
                    cart(),
                    doc,
                );
                let mut saved_cart = cart();
                if newer_cart {
                    saved_cart.items[0].quantity += 1;
                }
                let unrelated = Cart::new("another-convention".into(), "another-cart".into());
                let mut cache = dom.in_scope(ScopeId::APP, || {
                    use_context::<
                        Signal<std::collections::BTreeMap<String, crate::cart::browser::Restored>>,
                    >()
                });
                cache.write().insert(
                    saved_cart.convention_id.clone(),
                    crate::cart::browser::Restored {
                        cart: saved_cart.clone(),
                        notice: None,
                    },
                );
                cache.write().insert(
                    unrelated.convention_id.clone(),
                    crate::cart::browser::Restored {
                        cart: unrelated.clone(),
                        notice: None,
                    },
                );
                let html = until(&mut dom, "Congratulations!").await;
                assert!(html.contains("#CONVENTION-1234"));
                assert!(!html.contains("gid://shopify"));
                assert_eq!(
                    cache
                        .peek()
                        .get(&saved_cart.convention_id)
                        .map(|saved| &saved.cart),
                    newer_cart.then_some(&saved_cart)
                );
                assert_eq!(cache.peek()[&unrelated.convention_id].cart, unrelated);
            })
            .await;
            assert_eq!(requests.len(), 1);
            assert_eq!(
                *inspected.sent.borrow(),
                vec![json!({"status":"completed"})]
            );
        }
    }

    #[tokio::test]
    async fn fresh_validation_rejection_allows_cart_review_but_ambiguous_conflict_keeps_recovery() {
        for status in [400, 422, 409] {
            let attempt = attempt(CheckoutProvider::Stripe);
            let doc =
                TestDocument::new(vec![json!({"ok":true,"value":null}), boot(&attempt, true)]);
            let inspected = doc.clone();
            let ((), requests) = responses(
                &[(status, r#"{"error":"Checkout cannot proceed"}"#)],
                async {
                    let (mut dom, _) =
                        mounted(Screen::Checkout(cart(), Some(quote())), cart(), doc);
                    until(&mut dom, "Pay with PayPal").await;
                    input(&mut dom, "checkout-email", "input", "buyer@example.test");
                    click(&mut dom, "id", "checkout-card");
                    let expected = if status == 409 {
                        "We could not confirm this payment yet"
                    } else {
                        "Payment could not be started"
                    };
                    let html = until(&mut dom, expected).await;
                    assert_eq!(html.contains("Check existing payment"), status == 409);
                    assert_eq!(html.contains("checkout-email"), status != 409);
                    assert!(!html.contains("Congratulations"));
                },
            )
            .await;
            assert_eq!(requests.len(), 1);
            let disposition = if status == 409 {
                Value::Null
            } else {
                json!("rejected")
            };
            assert_eq!(
                *inspected.sent.borrow(),
                vec![json!({"status":disposition})]
            );
        }
    }

    #[tokio::test]
    async fn unusable_restored_create_payload_never_sends_a_replacement_purchase() {
        let attempt = attempt(CheckoutProvider::Stripe);
        let valid = boot(&attempt, true);
        let mut changed: Value = serde_json::from_str(
            valid["__eval_messages"][0]["value"]["payload"]
                .as_str()
                .expect("saved request"),
        )
        .expect("saved request JSON");
        changed["items"][0]["quantity"] = json!(999);
        for payload in [
            Value::Null,
            json!("invalid JSON"),
            json!(changed.to_string()),
        ] {
            let mut started = valid.clone();
            started["__eval_messages"][0]["value"]["payload"] = payload;
            let doc = TestDocument::new(vec![json!({"ok":true,"value":null}), started]);
            let inspected = doc.clone();
            let ((), requests) = responses(&[], async {
                let (mut dom, _) = mounted(Screen::Checkout(cart(), Some(quote())), cart(), doc);
                until(&mut dom, "Pay with PayPal").await;
                input(&mut dom, "checkout-email", "input", "buyer@example.test");
                click(&mut dom, "id", "checkout-card");
                let html = until(&mut dom, "We could not confirm this payment yet").await;
                assert!(html.contains("Check existing payment"));
            })
            .await;
            assert_eq!(requests, Vec::<String>::new());
            assert_eq!(*inspected.sent.borrow(), vec![json!({"status":null})]);
        }
    }

    #[tokio::test]
    async fn continue_payment_surfaces_unsafe_link_and_browser_navigation_failures() {
        for (url, expected, navigation_reply) in [
            (
                "https://untrusted.example/pay",
                "unsupported payment link",
                None,
            ),
            (
                "https://checkout.stripe.com/c/pay/original",
                "Unable to open the payment page",
                Some(Value::Null),
            ),
        ] {
            let attempt = attempt(CheckoutProvider::Stripe);
            let body = response(&attempt, CheckoutStatus::RequiresAction, Some(url));
            let mut replies = vec![
                json!({"ok":true,"value":attempt}),
                boot(&attempt, false),
                json!("https://convention.example/fair-a/checkout/return"),
            ];
            replies.extend(navigation_reply);
            let doc = TestDocument::new(replies);
            let inspected = doc.clone();
            let ((), requests) = responses(&[(200, &body)], async {
                let (mut dom, _) = mounted(
                    Screen::Route(format!(
                        "/fair-a/checkout/return?checkout_id={}",
                        attempt.operation_id
                    )),
                    cart(),
                    doc,
                );
                until(&mut dom, "Continue your payment").await;
                click(&mut dom, "__text", "Continue payment");
                let html = until(&mut dom, expected).await;
                assert!(!html.contains("Congratulations"));
            })
            .await;
            assert_eq!(requests.len(), 1);
            let scripts = inspected.scripts.borrow();
            assert!(scripts.iter().all(|script| {
                !script.contains("window.location.assign(\"https://untrusted.example")
            }));
        }
    }
}

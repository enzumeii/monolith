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
    assert!(html.contains("checkout-site-logo"));
    assert!(html.contains("#1234"));
    assert!(html.contains("Congratulations! Your order #1234 is confirmed."));
    assert!(html.contains("checkout-confirmation"));
    assert!(html.contains("Check your email for your order confirmation."));
    assert!(html.contains("<br"));
    assert!(html.contains("Show that email at the convention to pick up your order."));
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
        if status == CheckoutStatus::Processing {
            assert!(html.contains("checkout-site-logo"));
            assert!(html.contains("checkout-progress"));
            assert!(html.contains("role=\"status\""));
        }
    }
}

#[test]
fn unpaid_and_requires_action_render_distinct_payment_guidance() {
    assert!(render(CheckoutStatus::Unpaid, None).contains("This checkout is unpaid"));
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

#[test]
fn only_typed_temporary_request_errors_keep_payment_recovery_running() {
    use reqwest::StatusCode;
    assert!(
        request_error(
            &api::ApiError::http(StatusCode::SERVICE_UNAVAILABLE, "temporary"),
            false
        )
        .retryable
    );
    for error in [
        api::ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "local browser unavailable"),
        api::ApiError::new(StatusCode::BAD_GATEWAY, "invalid response"),
        api::ApiError::http(StatusCode::CONFLICT, "purchase differs"),
    ] {
        assert!(!request_error(&error, false).retryable);
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod mounted {
    use super::*;
    use crate::{
        api::tests::responses,
        pages::tests::{Screen, TestDocument, click, input, mounted, until},
    };
    use futures::FutureExt;
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
    fn boot(attempt: &Attempt, fresh: bool, note: &str) -> Value {
        let mut request = model::request(
            &cart(),
            &quote(),
            "buyer@example.test",
            note,
            attempt.provider,
        )
        .expect("valid checkout fixture");
        request.operation_id = attempt.operation_id.clone();
        json!({"__eval_messages":[{"ok":true,"value":{"attempt":attempt,"payload":serde_json::to_string(&request).expect("valid checkout fixture"),"fresh":fresh}},{"ok":true,"value":false}]})
    }
    fn payment_window() -> Value {
        json!({"__eval_messages":[{"ok":true,"value":null},{"ok":true,"value":null}]})
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
    async fn confirmation_progress_stays_visible_during_its_own_inspection() {
        #[component]
        fn Probe() -> Element {
            let attempt = use_context::<Attempt>();
            let inspection = use_resource(move || {
                let attempt = attempt.clone();
                async move { inspect(&attempt).await }
            });
            rsx! { ProcessingPayment {} if inspection.read().is_some() { p { "Inspection finished" } } }
        }
        fn app() -> Element {
            rsx! { ui::LoadingBoundary { Probe {} } }
        }
        let attempt = attempt(CheckoutProvider::Stripe);
        let document = TestDocument::new(vec![boot(&attempt, false, "")]);
        let server = crate::api::tests::delayed_loading_server(
            200,
            r#"{"checkout_id":"f79b7e64-8e36-4aca-9e21-ecb7cb513700","status":"processing","money":{"minor":2460,"currency":"USD"},"reservation_expires_at":null,"next_action":null}"#,
            false,
        );
        let mut settings = crate::config::tests::settings();
        settings.api.base_url = server.url.clone();
        let mut dom = VirtualDom::new(app)
            .with_root_context(attempt)
            .with_root_context(settings)
            .with_root_context(document as std::rc::Rc<dyn dioxus::document::Document>);
        dom.rebuild_in_place();
        let mut seen = false;
        for _ in 0..200 {
            let _ = dom.wait_for_work().now_or_never();
            dom.render_immediate_to_vec();
            if server.seen_rx.try_recv().is_ok() {
                seen = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert!(seen, "inspection reached the real controlled HTTP server");
        let before = dioxus_ssr::render(&dom);
        server.headers_tx.send(()).expect("release headers");
        for _ in 0..10 {
            let _ = dom.wait_for_work().now_or_never();
            dom.render_immediate_to_vec();
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        let during = dioxus_ssr::render(&dom);
        server.body_tx.send(()).expect("release body");
        until(&mut dom, "Inspection finished").await;
        server.server.join().expect("controlled server");
        for html in [before, during] {
            assert!(html.contains("checkout-progress"), "{html}");
            assert!(
                !html.contains("page-loading"),
                "confirmation must own its progress: {html}"
            );
            assert!(
                !html.contains("inert=\"inert\""),
                "confirmation must remain visible: {html}"
            );
        }
    }

    fn request_body(request: &str) -> Value {
        serde_json::from_str(request.split("\r\n\r\n").nth(1).expect("request body"))
            .expect("request JSON")
    }

    async fn until_released(dom: &mut VirtualDom, document: &TestDocument) {
        for _ in 0..200 {
            let _ = dom.wait_for_work().now_or_never();
            dom.render_immediate_to_vec();
            if !document.sent.borrow().is_empty() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("Checkout ownership was not released after verification");
    }

    async fn until_navigation(dom: &mut VirtualDom, document: &TestDocument) {
        for _ in 0..200 {
            let _ = dom.wait_for_work().now_or_never();
            dom.render_immediate_to_vec();
            if document
                .scripts
                .borrow()
                .iter()
                .any(|script| script.contains("window.location.assign"))
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("Checkout did not automatically open its original payment");
    }

    #[tokio::test]
    async fn saved_active_cart_automatically_opens_the_original_checkout_without_a_customer_action()
    {
        let attempt = attempt(CheckoutProvider::Stripe);
        let document = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            json!({"ok":true,"value":false}),
            json!(true),
        ]);
        let inspected = document.clone();
        let ((), requests) = responses(&[], async {
            let (mut dom, _) = mounted(Screen::Checkout(cart(), Some(quote())), cart(), document);
            until_navigation(&mut dom, &inspected).await;
            let html = dioxus_ssr::render(&dom);
            assert!(!html.contains("Check existing payment"));
            assert!(!html.contains("checkout-email"));
        })
        .await;
        assert_eq!(requests, Vec::<String>::new());
        assert!(
            inspected
                .scripts
                .borrow()
                .iter()
                .any(|script| script.contains(&format!(
                    "window.location.assign(\"/fair-a/checkout/return?checkout_id={}\")",
                    attempt.operation_id
                )))
        );
        assert!(
            !inspected
                .scripts
                .borrow()
                .iter()
                .any(|script| script.contains("setTimeout"))
        );
    }

    #[tokio::test]
    async fn saved_payment_handoff_keeps_the_cart_editor_disabled_until_navigation() {
        let attempt = attempt(CheckoutProvider::Stripe);
        let document = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            json!({"ok":true,"value":false}),
            json!({"__eval_pending":true}),
        ]);
        let inspected = document.clone();
        let mut catalogue = crate::api::tests::catalogue();
        catalogue.convention.id = cart().convention_id.clone();
        let catalogue_body = serde_json::to_string(&catalogue).expect("catalogue fixture");
        let quote_body = serde_json::to_string(&quote()).expect("quote fixture");
        let ((), requests) = responses(
            &[
                (200, &catalogue_body),
                (200, &quote_body),
                (200, &catalogue_body),
            ],
            async {
                let (mut dom, _) = mounted(
                    Screen::Cart(catalogue, cart().checkout_id.clone()),
                    cart(),
                    document,
                );
                until_navigation(&mut dom, &inspected).await;
                until(&mut dom, "Current total").await;
                let html = dioxus_ssr::render(&dom);
                assert!(
                    html.contains("class=\"portal-cart-editor\" disabled"),
                    "{html}"
                );
                assert!(!html.contains("Check existing payment"));
            },
        )
        .await;
        assert!(requests.iter().all(|request| !request.contains("/order/")));
    }

    #[tokio::test]
    async fn paused_saved_payment_keeps_the_cart_visible_until_explicit_resume() {
        let attempt = attempt(CheckoutProvider::Stripe);
        let document = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            json!({"ok":true,"value":true}),
            json!({"ok":true,"value":null}),
            json!(true),
        ]);
        let inspected = document.clone();
        let mut catalogue = crate::api::tests::catalogue();
        catalogue.convention.id = cart().convention_id.clone();
        let catalogue_body = serde_json::to_string(&catalogue).expect("catalogue fixture");
        let quote_body = serde_json::to_string(&quote()).expect("quote fixture");
        let ((), requests) = responses(
            &[
                (200, &catalogue_body),
                (200, &quote_body),
                (200, &catalogue_body),
            ],
            async {
                let (mut dom, captured) = mounted(
                    Screen::Cart(catalogue, cart().checkout_id.clone()),
                    cart(),
                    document,
                );
                until(&mut dom, "Resume payment").await;
                until(&mut dom, "Current total").await;
                let html = until(&mut dom, "class=\"portal-cart-editor\" disabled").await;
                assert!(html.contains("Your cart"));
                assert!(html.contains("class=\"portal-cart-editor\" disabled"));
                assert!(!html.contains("checkout-card"));
                assert!(
                    !inspected
                        .scripts
                        .borrow()
                        .iter()
                        .any(|s| s.contains("window.location.assign"))
                );
                assert_eq!(*captured.get().expect("cart context").cart.read(), cart());
                click(&mut dom, "__text", "Resume payment");
                until_navigation(&mut dom, &inspected).await;
            },
        )
        .await;
        assert!(requests.iter().all(|r| !r.contains("/order/")));
        let scripts = inspected.scripts.borrow();
        assert!(scripts.iter().any(|s| s.contains("\"action\":\"resume\"")));
        assert!(
            scripts
                .iter()
                .any(|s| s.contains(&attempt.operation_id) && s.contains("window.location.assign"))
        );
        assert!(!scripts.iter().any(|s| s.contains("\"action\":\"start\"")));
    }

    #[tokio::test]
    async fn cancel_checking_stops_the_pending_delay_and_returns_to_the_preserved_cart() {
        let attempt = attempt(CheckoutProvider::Stripe);
        let pending = response(&attempt, CheckoutStatus::Processing, None);
        let document = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            boot(&attempt, false, ""),
            json!({"__eval_pending":true}),
            json!({"ok":true,"value":null}),
            json!(true),
        ]);
        let inspected = document.clone();
        let ((), requests) = responses(&[(200, &pending)], async {
            let (mut dom, captured) = mounted(
                Screen::Route(format!(
                    "/fair-a/checkout/return?checkout_id={}",
                    attempt.operation_id
                )),
                cart(),
                document,
            );
            until_released(&mut dom, &inspected).await;
            until(&mut dom, "Cancel and return to cart").await;
            click(&mut dom, "__text", "Cancel and return to cart");
            until_navigation(&mut dom, &inspected).await;
            for _ in 0..10 {
                let _ = dom.wait_for_work().now_or_never();
                dom.render_immediate_to_vec();
            }
            assert_eq!(*captured.get().expect("cart context").cart.read(), cart());
        })
        .await;
        assert_eq!(requests.len(), 1);
        assert!(requests[0].contains("/verify "));
        let scripts = inspected.scripts.borrow();
        assert!(scripts.iter().any(|s| s.contains("\"action\":\"pause\"")));
        assert!(scripts.iter().any(|s| s.contains(&format!(
            "window.location.assign(\"/fair-a/cart/{}\")",
            cart().checkout_id
        ))));
        assert!(!scripts.iter().any(|s| s.contains("\"action\":\"start\"")));
    }

    #[tokio::test]
    async fn initial_checking_can_be_cancelled_before_the_first_verification_completes() {
        let attempt = attempt(CheckoutProvider::Paypal);
        let document = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            json!({"__eval_pending":true}),
            json!({"ok":true,"value":null}),
            json!(true),
        ]);
        let inspected = document.clone();
        let ((), requests) = responses(&[], async {
            let (mut dom, _) = mounted(
                Screen::Route(format!(
                    "/fair-a/checkout/return?checkout_id={}",
                    attempt.operation_id
                )),
                cart(),
                document,
            );
            until(&mut dom, "Cancel and return to cart").await;
            for _ in 0..20 {
                let _ = dom.wait_for_work().now_or_never();
                dom.render_immediate_to_vec();
                if inspected
                    .scripts
                    .borrow()
                    .iter()
                    .any(|s| s.contains("\"action\":\"recover\""))
                {
                    break;
                }
            }
            assert!(
                inspected
                    .scripts
                    .borrow()
                    .iter()
                    .any(|s| s.contains("\"action\":\"recover\""))
            );
            click(&mut dom, "__text", "Cancel and return to cart");
            until_navigation(&mut dom, &inspected).await;
        })
        .await;
        assert_eq!(requests, Vec::<String>::new());
        assert!(
            inspected
                .scripts
                .borrow()
                .iter()
                .any(|s| s.contains("\"action\":\"pause\""))
        );
    }

    #[tokio::test]
    async fn lost_create_response_automatically_hands_off_then_replays_the_same_saved_purchase() {
        let attempt = attempt(CheckoutProvider::Stripe);
        let document = TestDocument::new(vec![
            json!({"ok":true,"value":null}),
            payment_window(),
            boot(&attempt, true, "  Keep flat.\n🐕 "),
            json!({"ok":true,"value":false}),
            json!(true),
            json!(true),
        ]);
        let inspected = document.clone();
        let ((), original_requests) =
            responses(&[(503, r#"{"error":"Temporary failure"}"#)], async {
                let (mut dom, _) =
                    mounted(Screen::Checkout(cart(), Some(quote())), cart(), document);
                until(&mut dom, "Pay USD 24.60 via PayPal").await;
                input(&mut dom, "checkout-email", "input", "buyer@example.test");
                input(&mut dom, "checkout-note", "input", "  Keep flat.\n🐕 ");
                click(&mut dom, "id", "checkout-card");
                until_navigation(&mut dom, &inspected).await;
                assert!(!dioxus_ssr::render(&dom).contains("Check existing payment"));
            })
            .await;
        assert_eq!(original_requests.len(), 1);
        let sent = request_body(&original_requests[0]);
        assert_eq!(sent["note"], "  Keep flat.\n🐕 ");
        {
            let scripts = inspected.scripts.borrow();
            let delay = scripts
                .iter()
                .position(|script| script.contains("setTimeout(resolve, 5000)"))
                .expect("first failure waits five seconds");
            let navigation = scripts
                .iter()
                .position(|script| {
                    script.contains(&format!(
                        "/fair-a/checkout/return?checkout_id={}",
                        attempt.operation_id
                    ))
                })
                .expect("same checkout route");
            assert!(delay < navigation);
        }
        let body = response(&attempt, CheckoutStatus::Processing, None);
        let document = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            boot(&attempt, false, "  Keep flat.\n🐕 "),
            json!({"__eval_pending":true}),
        ]);
        let inspected = document.clone();
        let ((), recovered_requests) = responses(
            &[(404, r#"{"error":"Checkout not found"}"#), (200, &body)],
            async {
                let (mut dom, _) = mounted(
                    Screen::Route(format!(
                        "/fair-a/checkout/return?checkout_id={}",
                        attempt.operation_id
                    )),
                    cart(),
                    document,
                );
                until_released(&mut dom, &inspected).await;
                until(&mut dom, "Your payment is being checked").await;
            },
        )
        .await;
        assert_eq!(recovered_requests.len(), 2);
        assert!(
            recovered_requests[0]
                .starts_with(&format!("POST /v1/order/{}/verify ", attempt.operation_id))
        );
        assert!(recovered_requests[1].starts_with("POST /v1/order/checkout "));
        assert_eq!(
            original_requests[0].split("\r\n\r\n").nth(1),
            recovered_requests[1].split("\r\n\r\n").nth(1)
        );
        assert!(
            original_requests
                .iter()
                .chain(&recovered_requests)
                .all(|request| request.contains(&attempt.secret))
        );
        assert!(
            !inspected
                .scripts
                .borrow()
                .iter()
                .any(|script| script.contains("\"action\":\"start\""))
        );
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
                let html = until(
                    &mut dom,
                    if quote.is_some() {
                        "Pay USD 24.60 via PayPal"
                    } else {
                        "Pay via PayPal"
                    },
                )
                .await;
                assert_eq!(html.matches("checkout-payment-button").count(), 2);
                assert!(html.contains("checkout-payment-methods"));
                if quote.is_some() {
                    assert!(html.contains("Pay USD 24.60 by credit card"));
                }
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
    async fn optional_note_is_below_email_and_invalid_note_disables_both_providers() {
        responses(&[], async {
            let (mut dom, _) = mounted(
                Screen::Checkout(cart(), Some(quote())),
                cart(),
                TestDocument::new(vec![json!({"ok":true,"value":null})]),
            );
            let html = until(&mut dom, "Order note (optional)").await;
            assert!(
                html.find("checkout-email").expect("email")
                    < html.find("checkout-note").expect("note")
            );
            assert!(html.contains("<textarea"));
            assert!(html.contains("2,000 characters"));
            input(&mut dom, "checkout-email", "input", "buyer@example.test");
            for note in ["🐕".repeat(2001), "unsafe\0".into()] {
                input(&mut dom, "checkout-note", "input", &note);
                let html = dioxus_ssr::render(&dom);
                assert!(html.contains("role=\"alert\""));
                for id in ["checkout-paypal", "checkout-card"] {
                    let button = html
                        .split(&format!("id=\"{id}\""))
                        .nth(1)
                        .expect("payment button")
                        .split('>')
                        .next()
                        .expect("button attributes");
                    assert!(button.contains("disabled"), "{button}");
                }
            }
            input(&mut dom, "checkout-note", "input", &"🐕".repeat(2000));
            let html = dioxus_ssr::render(&dom);
            assert!(!html.contains("Keep your order note"));
            assert!(!html.contains("unsupported control"));
        })
        .await;
    }

    fn assert_payment_handoff(document: &TestDocument, url: &str) {
        let scripts = document.scripts.borrow();
        assert!(
            !scripts
                .iter()
                .any(|script| script.contains("window.location.assign"))
        );
        assert_eq!(
            scripts
                .iter()
                .filter(|script| script.contains("window.open"))
                .count(),
            1
        );
        let reserve = scripts
            .iter()
            .position(|script| script.contains("window.open"))
            .expect("reserved window");
        let checkpoint = scripts
            .iter()
            .position(|script| script.contains("\"action\":\"start\""))
            .expect("checkpoint");
        assert!(reserve < checkpoint);
        assert!(
            document
                .sent
                .borrow()
                .iter()
                .any(|command| command == &json!({"action":"open","url":url}))
        );
        assert!(
            !scripts
                .iter()
                .any(|script| script.contains("\"action\":\"recover\""))
        );
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
                payment_window(),
                boot(&attempt, true, "  Pack <b>flat</b>.\n🐕 "),
                json!("http://localhost:8085/fair-a/cart/cart"),
                json!({"__eval_pending":true}),
            ]);
            let inspect = doc.clone();
            let ((), requests) = responses(&[(200, &body)], async {
                let (mut dom, _) = mounted(Screen::Checkout(cart(), Some(quote())), cart(), doc);
                until(&mut dom, "Pay USD 24.60 via PayPal").await;
                input(&mut dom, "checkout-email", "input", "buyer@example.test");
                input(
                    &mut dom,
                    "checkout-note",
                    "input",
                    "  Pack <b>flat</b>.\n🐕 ",
                );
                let html = dioxus_ssr::render(&dom);
                assert!(html.contains("&#60;b&#62;flat&#60;/b&#62;"), "{html}");
                assert!(!html.contains("<b>flat</b>"));
                let label = match provider {
                    CheckoutProvider::Paypal => "checkout-paypal",
                    CheckoutProvider::Stripe => "checkout-card",
                };
                click(&mut dom, "id", label);
                assert!(
                    inspect
                        .scripts
                        .borrow()
                        .iter()
                        .any(|script| script.contains("window.open")),
                    "window reserved during activation"
                );
                click(&mut dom, "id", label);
                until(&mut dom, "Your payment is being checked").await;
                for _ in 0..10 {
                    let _ = dom.wait_for_work().now_or_never();
                    dom.render_immediate_to_vec();
                }
                let html = dioxus_ssr::render(&dom);
                assert!(html.contains("Your payment is being checked"));
                assert!(html.contains("Cancel and return to cart"));
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
            let sent = request_body(&requests[0]);
            assert_eq!(sent["email"], "buyer@example.test");
            assert_eq!(sent["note"], "  Pack <b>flat</b>.\n🐕 ");
            let scripts = inspect.scripts.borrow();
            let start = scripts
                .iter()
                .find(|script| script.contains("\"action\":\"start\""))
                .expect("saved checkout start");
            assert!(start.contains("Pack <b>flat</b>"));
            assert_eq!(sent["items"][0]["expected_unit_price"], 1230);
            assert_eq!(
                sent["provider"],
                serde_json::to_value(provider).expect("valid checkout fixture")
            );
            assert_payment_handoff(&inspect, url);
            assert!(
                !requests
                    .iter()
                    .any(|request| request.to_ascii_lowercase().contains("cookie:"))
            );
        }
    }

    #[tokio::test]
    async fn blocked_payment_window_releases_the_button_without_creating_a_checkout() {
        let document = TestDocument::new(vec![
            json!({"ok":true,"value":null}),
            json!({"ok":false,"error":"Allow pop-ups for this shop, then try opening your payment again."}),
        ]);
        let inspected = document.clone();
        let ((), requests) = responses(&[], async {
            let (mut dom, _) = mounted(Screen::Checkout(cart(), Some(quote())), cart(), document);
            until(&mut dom, "Pay USD 24.60 by credit card").await;
            input(&mut dom, "checkout-email", "input", "buyer@example.test");
            click(&mut dom, "id", "checkout-card");
            let html = until(&mut dom, "Allow pop-ups").await;
            assert!(!html.contains("Preparing your secure payment"));
            assert!(
                !html
                    .split("id=\"checkout-card\"")
                    .nth(1)
                    .expect("card button")
                    .split('>')
                    .next()
                    .expect("attributes")
                    .contains("disabled")
            );
        })
        .await;
        assert_eq!(requests, Vec::<String>::new());
        assert!(
            !inspected
                .scripts
                .borrow()
                .iter()
                .any(|s| s.contains("\"action\":\"start\""))
        );
    }

    #[tokio::test]
    async fn original_page_keeps_checking_an_open_provider_until_verified_completion() {
        for completes in [false, true] {
            let attempt = attempt(CheckoutProvider::Stripe);
            let action = response(
                &attempt,
                CheckoutStatus::RequiresAction,
                Some("https://checkout.stripe.com/c/pay/original"),
            );
            let mut paid: Value =
                serde_json::from_str(&response(&attempt, CheckoutStatus::Completed, None))
                    .expect("response");
            paid["order"] = json!({"id":"gid://shopify/Order/1234","name":"#1234"});
            let paid = paid.to_string();
            let mut replies = vec![
                json!({"ok":true,"value":null}),
                payment_window(),
                boot(&attempt, true, ""),
                json!("https://convention.example/fair-a/cart/original"),
                json!(true),
                boot(&attempt, false, ""),
                if completes {
                    json!(true)
                } else {
                    json!({"__eval_pending":true})
                },
            ];
            let mut outcomes = vec![(200, action.as_str()), (200, action.as_str())];
            if completes {
                replies.push(boot(&attempt, false, ""));
                outcomes.push((200, paid.as_str()));
            }
            let document = TestDocument::new(replies);
            let inspected = document.clone();
            let ((), requests) = responses(&outcomes, async {
                let (mut dom, _) =
                    mounted(Screen::Checkout(cart(), Some(quote())), cart(), document);
                until(&mut dom, "Pay USD 24.60 by credit card").await;
                input(&mut dom, "checkout-email", "input", "buyer@example.test");
                click(&mut dom, "id", "checkout-card");
                if completes {
                    let html = until(&mut dom, "Congratulations!").await;
                    assert!(html.contains("#1234"));
                    assert!(!html.contains("checkout-progress"));
                } else {
                    for _ in 0..200 {
                        let _ = dom.wait_for_work().now_or_never();
                        dom.render_immediate_to_vec();
                        if inspected
                            .sent
                            .borrow()
                            .iter()
                            .filter(|value| *value == &json!({"status":"requires_action"}))
                            .count()
                            == 2
                        {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                    assert_eq!(
                        inspected
                            .sent
                            .borrow()
                            .iter()
                            .filter(|value| *value == &json!({"status":"requires_action"}))
                            .count(),
                        2
                    );
                    let html = dioxus_ssr::render(&dom);
                    assert!(html.contains("Your payment is being checked"));
                    assert!(!html.contains("Continue payment"));
                    assert!(html.contains("Cancel and return to cart"));
                }
            })
            .await;
            assert_eq!(requests.len(), if completes { 3 } else { 2 });
            assert!(requests[0].contains("POST /v1/order/checkout "));
            assert!(requests[1..].iter().all(|r| {
                r.contains(&format!("/order/{}/verify ", attempt.operation_id))
                    && r.contains(&attempt.secret)
            }));
            assert!(
                !inspected
                    .scripts
                    .borrow()
                    .iter()
                    .any(|s| s.contains("window.location.assign"))
            );
            assert_eq!(
                inspected
                    .scripts
                    .borrow()
                    .iter()
                    .filter(|s| s.contains("window.open"))
                    .count(),
                1
            );
        }
    }

    #[tokio::test]
    async fn failed_pause_keeps_cancel_action_retryable_without_resuming_checks() {
        let attempt = attempt(CheckoutProvider::Paypal);
        let pending = response(&attempt, CheckoutStatus::Processing, None);
        let document = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            boot(&attempt, false, ""),
            json!({"__eval_pending":true}),
            json!({"ok":false,"error":"Unable to save checking preference. Try again."}),
            json!({"ok":true,"value":null}),
            json!(true),
        ]);
        let inspected = document.clone();
        let ((), requests) = responses(&[(200, &pending)], async {
            let (mut dom, _) = mounted(
                Screen::Route(format!(
                    "/fair-a/checkout/return?checkout_id={}",
                    attempt.operation_id
                )),
                cart(),
                document,
            );
            until_released(&mut dom, &inspected).await;
            until(&mut dom, "Cancel and return to cart").await;
            click(&mut dom, "__text", "Cancel and return to cart");
            let html = until(&mut dom, "Unable to save checking preference").await;
            assert!(html.contains("Cancel and return to cart"));
            assert!(
                !inspected
                    .scripts
                    .borrow()
                    .iter()
                    .any(|s| s.contains("window.location.assign"))
            );
            click(&mut dom, "__text", "Cancel and return to cart");
            until_navigation(&mut dom, &inspected).await;
        })
        .await;
        assert_eq!(requests.len(), 1);
    }

    #[tokio::test]
    async fn return_with_paypal_query_parameters_verifies_without_catalogue_and_never_claims_http_success()
     {
        let attempt = attempt(CheckoutProvider::Paypal);
        let body = response(&attempt, CheckoutStatus::Processing, None);
        let doc = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            boot(&attempt, false, ""),
            json!({"__eval_pending":true}),
        ]);
        let inspected = doc.clone();
        let ((), requests) = responses(&[(200, &body)], async {
            let path = format!(
                "/fair-a/checkout/return?checkout_id={}&token=untrusted&PayerID=untrusted",
                attempt.operation_id
            );
            let (mut dom, _) = mounted(Screen::Route(path), cart(), doc);
            until_released(&mut dom, &inspected).await;
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
                let mut expired_boot = boot(&attempt, false, "");
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
        let original = boot(&attempt, false, "");
        let expected_payload = original["__eval_messages"][0]["value"]["payload"]
            .as_str()
            .expect("valid checkout fixture")
            .to_owned();
        let doc = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            original,
            json!({"__eval_pending":true}),
        ]);
        let inspected = doc.clone();
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
                until_released(&mut dom, &inspected).await;
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
            payment_window(),
            boot(&attempt, false, ""),
            json!(true),
        ]);
        let inspected = doc.clone();
        let ((), requests) = responses(&[(200, &body)], async {
            let (mut dom, _) = mounted(Screen::Checkout(cart(), Some(quote())), cart(), doc);
            until(&mut dom, "Pay USD 24.60 via PayPal").await;
            input(&mut dom, "checkout-email", "input", "buyer@example.test");
            click(&mut dom, "id", "checkout-paypal");
            until_navigation(&mut dom, &inspected).await;
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
    async fn temporary_failure_automatically_rechecks_the_same_payment_without_manual_action() {
        let attempt = attempt(CheckoutProvider::Paypal);
        let body = response(&attempt, CheckoutStatus::Unpaid, None);
        let doc = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            boot(&attempt, false, ""),
            json!(true),
            boot(&attempt, false, ""),
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
                let html = until(&mut dom, "Payment could not be completed").await;
                assert!(html.contains("Return to cart"));
                assert!(!html.contains("Check status"));
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
    async fn processing_continues_beyond_twelve_checks_and_stops_on_authoritative_completion() {
        let attempt = attempt(CheckoutProvider::Stripe);
        let pending = response(&attempt, CheckoutStatus::Processing, None);
        let mut paid: Value =
            serde_json::from_str(&response(&attempt, CheckoutStatus::Completed, None))
                .expect("completed response fixture");
        paid["order"] = json!({"id":"gid://shopify/Order/1234","name":"#1234"});
        let paid = paid.to_string();
        let mut replies = vec![json!({"ok":true,"value":attempt})];
        for _ in 0..14 {
            replies.extend([boot(&attempt, false, ""), json!(true)]);
        }
        replies.push(boot(&attempt, false, ""));
        let doc = TestDocument::new(replies);
        let inspected = doc.clone();
        let mut outcomes = vec![(200, pending.as_str()); 14];
        outcomes.push((200, paid.as_str()));
        let ((), requests) = responses(&outcomes, async {
            let (mut dom, _) = mounted(
                Screen::Route(format!(
                    "/fair-a/checkout/return?checkout_id={}",
                    attempt.operation_id
                )),
                cart(),
                doc,
            );
            let html = until(&mut dom, "Congratulations!").await;
            assert!(html.contains("checkout-site-logo"));
            assert!(!html.contains("Check status"));
        })
        .await;
        assert_eq!(requests.len(), 15);
        assert!(requests.iter().all(|r| {
            r.contains(&format!("/order/{}/verify ", attempt.operation_id))
                && r.contains(&attempt.secret)
        }));
        assert_eq!(
            inspected
                .scripts
                .borrow()
                .iter()
                .filter(|s| s.contains("setTimeout"))
                .count(),
            14
        );
    }

    #[tokio::test]
    async fn transient_errors_use_capped_backoff_and_pending_resets_the_delay() {
        let attempt = attempt(CheckoutProvider::Paypal);
        let pending = response(&attempt, CheckoutStatus::Processing, None);
        let unpaid = response(&attempt, CheckoutStatus::Unpaid, None);
        let statuses = [503, 408, 429, 502, 503];
        let mut replies = vec![json!({"ok":true,"value":attempt})];
        for _ in 0..8 {
            replies.extend([boot(&attempt, false, ""), json!(true)]);
        }
        replies.push(boot(&attempt, false, ""));
        let doc = TestDocument::new(replies);
        let inspected = doc.clone();
        let mut outcomes: Vec<_> = statuses
            .into_iter()
            .map(|s| (s, r#"{"error":"Temporary"}"#))
            .collect();
        outcomes.extend([
            (200, pending.as_str()),
            (503, r#"{"error":"Temporary"}"#),
            (503, r#"{"error":"Temporary"}"#),
            (200, unpaid.as_str()),
        ]);
        let ((), requests) = responses(&outcomes, async {
            let (mut dom, _) = mounted(
                Screen::Route(format!(
                    "/fair-a/checkout/return?checkout_id={}",
                    attempt.operation_id
                )),
                cart(),
                doc,
            );
            let html = until(&mut dom, "Payment could not be completed").await;
            assert!(!html.contains("Check status"));
            assert!(!html.contains("We could not confirm this payment yet"));
        })
        .await;
        assert_eq!(requests.len(), 9);
        let delays: Vec<_> = inspected
            .scripts
            .borrow()
            .iter()
            .filter(|s| s.contains("setTimeout"))
            .cloned()
            .collect();
        for (script, millis) in delays
            .iter()
            .zip([5000, 10000, 20000, 30000, 30000, 5000, 5000, 10000])
        {
            assert!(
                script.contains(&format!("setTimeout(resolve, {millis})")),
                "{script}"
            );
        }
        assert_eq!(delays.len(), 8);
    }

    #[tokio::test]
    async fn polling_stops_on_terminal_http_storage_or_response_identity_errors() {
        let attempt = attempt(CheckoutProvider::Stripe);
        for status in [400, 401, 403, 404, 409, 422] {
            let mut started = boot(&attempt, false, "");
            started["__eval_messages"][0]["value"]["payload"] = Value::Null;
            let doc = TestDocument::new(vec![json!({"ok":true,"value":attempt}), started]);
            let inspected = doc.clone();
            let ((), requests) = responses(
                &[(
                    status,
                    r#"{"error":"Stripe requires a minimum payment of THB 10.00"}"#,
                )],
                async {
                    let (mut dom, _) = mounted(
                        Screen::Route(format!(
                            "/fair-a/checkout/return?checkout_id={}",
                            attempt.operation_id
                        )),
                        cart(),
                        doc,
                    );
                    let html = until(&mut dom, "contact the shop").await;
                    assert!(!html.contains("THB 10.00"));
                    assert!(!html.contains("Check status"));
                },
            )
            .await;
            assert_eq!(requests.len(), 1);
            assert!(
                !inspected
                    .scripts
                    .borrow()
                    .iter()
                    .any(|s| s.contains("setTimeout"))
            );
        }
        let mut wrong: Value =
            serde_json::from_str(&response(&attempt, CheckoutStatus::Processing, None))
                .expect("processing response fixture");
        wrong["checkout_id"] = json!("e95e0949-689b-4d89-9396-7dc7277e5d6f");
        let wrong = wrong.to_string();
        let doc = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            boot(&attempt, false, ""),
        ]);
        let inspected = doc.clone();
        let ((), requests) = responses(&[(200, &wrong)], async {
            let (mut dom, _) = mounted(
                Screen::Route(format!(
                    "/fair-a/checkout/return?checkout_id={}",
                    attempt.operation_id
                )),
                cart(),
                doc,
            );
            until(&mut dom, "different checkout").await;
        })
        .await;
        assert_eq!(requests.len(), 1);
        assert!(
            !inspected
                .scripts
                .borrow()
                .iter()
                .any(|s| s.contains("setTimeout"))
        );
    }

    #[tokio::test]
    async fn unsafe_browser_recovery_stops_without_network_or_automatic_retry() {
        let attempt = attempt(CheckoutProvider::Stripe);
        let doc = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            json!({"ok":false,"error":"Payment recovery is unavailable in this browser."}),
        ]);
        let inspected = doc.clone();
        let ((), requests) = responses(&[], async {
            let (mut dom, _) = mounted(
                Screen::Route(format!(
                    "/fair-a/checkout/return?checkout_id={}",
                    attempt.operation_id
                )),
                cart(),
                doc,
            );
            let html = until(&mut dom, "Payment recovery is unavailable").await;
            assert!(!html.contains("checkout-progress"));
            assert!(!html.contains("Check status"));
        })
        .await;
        assert_eq!(requests, Vec::<String>::new());
        assert!(
            !inspected
                .scripts
                .borrow()
                .iter()
                .any(|s| s.contains("setTimeout"))
        );
    }

    #[tokio::test]
    async fn unavailable_browser_delay_stops_payment_polling_with_specific_guidance() {
        let attempt = attempt(CheckoutProvider::Stripe);
        let pending = response(&attempt, CheckoutStatus::Processing, None);
        let document = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            boot(&attempt, false, ""),
            json!(false),
        ]);
        let inspected = document.clone();
        let ((), requests) = responses(&[(200, &pending)], async {
            let (mut dom, _) = mounted(
                Screen::Route(format!(
                    "/fair-a/checkout/return?checkout_id={}",
                    attempt.operation_id
                )),
                cart(),
                document,
            );
            let html = until(
                &mut dom,
                "Automatic payment recovery is unavailable in this browser",
            )
            .await;
            assert!(html.contains("contact the shop"));
            assert!(!html.contains("checkout-progress"));
            assert!(!html.contains("Check status"));
        })
        .await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            inspected
                .scripts
                .borrow()
                .iter()
                .filter(|script| script.contains("setTimeout"))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn disposing_processing_page_cancels_the_pending_delay_and_further_checks() {
        let attempt = attempt(CheckoutProvider::Stripe);
        let pending = response(&attempt, CheckoutStatus::Processing, None);
        let doc = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            boot(&attempt, false, ""),
            json!({"__eval_pending":true}),
        ]);
        let inspected = doc.clone();
        let ((), requests) = responses(&[(200, &pending)], async {
            let (mut dom, _) = mounted(
                Screen::Route(format!(
                    "/fair-a/checkout/return?checkout_id={}",
                    attempt.operation_id
                )),
                cart(),
                doc,
            );
            until(&mut dom, "Your payment is being checked").await;
            for _ in 0..20 {
                let _ = dom.wait_for_work().now_or_never();
                dom.render_immediate_to_vec();
                if inspected
                    .scripts
                    .borrow()
                    .iter()
                    .any(|s| s.contains("setTimeout"))
                {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            assert!(
                inspected
                    .scripts
                    .borrow()
                    .iter()
                    .any(|s| s.contains("setTimeout"))
            );
            drop(dom);
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        })
        .await;
        assert_eq!(requests.len(), 1);
        assert_eq!(
            *inspected.sent.borrow(),
            vec![json!({"status":"processing"})]
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
                boot(&attempt, false, ""),
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
            let doc = TestDocument::new(vec![
                json!({"ok":true,"value":null}),
                payment_window(),
                boot(&attempt, true, ""),
            ]);
            let inspected = doc.clone();
            let ((), requests) = responses(
                &[(
                    status,
                    r#"{"error":"Stripe requires a minimum payment of THB 10.00"}"#,
                )],
                async {
                    let (mut dom, _) =
                        mounted(Screen::Checkout(cart(), Some(quote())), cart(), doc);
                    until(&mut dom, "Pay USD 24.60 via PayPal").await;
                    input(&mut dom, "checkout-email", "input", "buyer@example.test");
                    click(&mut dom, "id", "checkout-card");
                    let expected = if status == 409 {
                        "Payment recovery could not continue"
                    } else {
                        "Payment could not be started"
                    };
                    let html = until(&mut dom, expected).await;
                    assert!(!html.contains("THB 10.00"));
                    assert!(!html.contains("Check existing payment"));
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
                inspected
                    .sent
                    .borrow()
                    .iter()
                    .filter(|value| value.get("status").is_some())
                    .cloned()
                    .collect::<Vec<_>>(),
                vec![json!({"status":disposition})]
            );
        }
    }

    #[tokio::test]
    async fn unusable_restored_create_payload_never_sends_a_replacement_purchase() {
        let attempt = attempt(CheckoutProvider::Stripe);
        let valid = boot(&attempt, true, "");
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
            let doc = TestDocument::new(vec![
                json!({"ok":true,"value":null}),
                payment_window(),
                started,
            ]);
            let inspected = doc.clone();
            let ((), requests) = responses(&[], async {
                let (mut dom, _) = mounted(Screen::Checkout(cart(), Some(quote())), cart(), doc);
                until(&mut dom, "Pay USD 24.60 via PayPal").await;
                input(&mut dom, "checkout-email", "input", "buyer@example.test");
                click(&mut dom, "id", "checkout-card");
                let html = until(&mut dom, "original checkout details").await;
                assert!(!html.contains("Check existing payment"));
            })
            .await;
            assert_eq!(requests, Vec::<String>::new());
            assert_eq!(
                inspected
                    .sent
                    .borrow()
                    .iter()
                    .filter(|value| value.get("status").is_some())
                    .cloned()
                    .collect::<Vec<_>>(),
                vec![json!({"status":null})]
            );
        }
    }

    #[tokio::test]
    async fn cancelled_gateway_requires_an_explicit_continue_before_reopening_the_original_payment()
    {
        let attempt = attempt(CheckoutProvider::Stripe);
        let body = response(
            &attempt,
            CheckoutStatus::RequiresAction,
            Some("https://checkout.stripe.com/c/pay/original"),
        );
        let document = TestDocument::new(vec![
            json!({"ok":true,"value":attempt}),
            boot(&attempt, false, ""),
            payment_window(),
            json!("https://convention.example/fair-a/checkout/cancel"),
            json!({"__eval_pending":true}),
        ]);
        let inspected = document.clone();
        let ((), requests) = responses(&[(200, &body)], async {
            let (mut dom, _) = mounted(
                Screen::Route(format!(
                    "/fair-a/checkout/cancel?checkout_id={}",
                    attempt.operation_id
                )),
                cart(),
                document,
            );
            until(&mut dom, "Continue your payment").await;
            for _ in 0..10 {
                let _ = dom.wait_for_work().now_or_never();
                dom.render_immediate_to_vec();
            }
            assert!(
                !inspected
                    .scripts
                    .borrow()
                    .iter()
                    .any(|script| script.contains("window.location.assign"))
            );
            click(&mut dom, "__text", "Continue payment");
            until(&mut dom, "Your payment is being checked").await;
        })
        .await;
        assert_eq!(requests.len(), 1);
        assert!(inspected.sent.borrow().iter().any(|command| command
            == &json!({"action":"open","url":"https://checkout.stripe.com/c/pay/original"})));
        assert!(
            !inspected
                .scripts
                .borrow()
                .iter()
                .any(|script| script.contains("window.location.assign"))
        );
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
                "The payment window was closed",
                Some(
                    json!({"ok":false,"error":"The payment window was closed. Resume this payment to open it again."}),
                ),
            ),
        ] {
            let attempt = attempt(CheckoutProvider::Stripe);
            let body = response(&attempt, CheckoutStatus::RequiresAction, Some(url));
            let replies = vec![
                json!({"ok":true,"value":attempt}),
                boot(&attempt, false, ""),
                json!({"__eval_messages":[{"ok":true,"value":null},navigation_reply.clone().unwrap_or(json!({"ok":true,"value":null}))]}),
                json!("https://convention.example/fair-a/checkout/return"),
            ];

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

#[test]
fn provider_minimum_details_are_hidden_without_releasing_an_uncertain_attempt() {
    let error = api::ApiError::new(
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "Stripe requires a minimum payment of THB 10.00",
    );
    let fresh = request_error(&error, true);
    assert!(fresh.message.starts_with("Payment could not be started"));
    assert!(!fresh.message.contains("Stripe"));
    assert!(!fresh.message.contains("10.00"));
    assert_eq!(fresh.disposition, Some("rejected"));
    assert!(!fresh.retryable);
    let recovery = request_error(&error, false);
    assert!(
        recovery
            .message
            .starts_with("Payment recovery could not continue")
    );
    assert!(!recovery.message.contains("Stripe"));
    assert!(!recovery.message.contains("10.00"));
    assert_eq!(recovery.disposition, None);
    assert!(!recovery.retryable);
}

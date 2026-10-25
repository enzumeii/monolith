use super::*;

#[test]
fn summary_preserves_exact_large_decimal_strings_and_has_no_payment_action() {
    fn app() -> Element {
        rsx! { QuoteSummary { quote: domain::dto::ConventionQuote { currency_code: "USD".into(), items: vec![], subtotal: "9007199254740993.12".into(), shipping: "0.00".into(), total: "9007199254740993.12".into() } } }
    }
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("USD 9007199254740993.12"));
    assert!(!html.contains("Shipping"));
    assert!(!html.contains("USD 0.00"));
    assert!(!html.contains("button"));
}

#[cfg(not(target_arch = "wasm32"))]
mod mounted {
    use super::*;
    use crate::{
        api::tests::{catalogue, responses},
        pages::tests::{CHECKOUT, Screen, TestDocument, click, empty_cart, input, mounted, until},
    };
    use futures::FutureExt;

    fn populated() -> Cart {
        let mut cart = empty_cart();
        cart.add(&crate::tests::variant(), 2)
            .expect("add fixture cart selection");
        cart
    }
    fn encoded() -> String {
        serde_json::to_string(&catalogue()).expect("serialize catalogue fixture")
    }
    fn quote(price: &str, quantity: u32) -> String {
        let total = match (price, quantity) {
            ("12.30", 2) => "24.60",
            ("12.30", 1) => "12.30",
            ("14.00", 2) => "28.00",
            _ => panic!("Unexpected quote fixture"),
        };
        serde_json::json!({"currency_code":"USD","items":[{"variant_id":crate::tests::variant().id,"quantity":quantity,"unit_price":price,"line_total":total}],"subtotal":total,"shipping":"0.00","total":total}).to_string()
    }

    #[tokio::test]
    async fn current_catalogue_hides_disabled_payment_methods() {
        for (methods, paypal, card) in [
            (serde_json::json!(["stripe"]), false, true),
            (serde_json::json!(["paypal"]), true, false),
            (serde_json::json!(["stripe", "paypal"]), true, true),
            (serde_json::json!([]), false, false),
        ] {
            let mut value = serde_json::to_value(catalogue()).expect("catalogue fixture");
            value["payment_methods"] = methods;
            let data = value.to_string();
            let quoted = quote("12.30", 2);
            let ((), requests) = responses(&[(200, &data), (200, &quoted), (200, &data)], async {
                let (mut dom, _) = mounted(
                    Screen::Cart(catalogue(), CHECKOUT.into()),
                    populated(),
                    TestDocument::new(vec![]),
                );
                let html = until(&mut dom, "Current total").await;
                assert_eq!(html.contains("id=\"checkout-paypal\""), paypal, "{html}");
                assert_eq!(html.contains("id=\"checkout-card\""), card, "{html}");
            })
            .await;
            assert_eq!(requests.len(), 3);
        }
    }

    #[tokio::test]
    async fn cart_resources_quote_edit_remove_and_never_render_stale_totals() {
        let data = encoded();
        let quoted = quote("12.30", 2);
        let single = quote("12.30", 1);
        let empty=serde_json::json!({"currency_code":"USD","items":[],"subtotal":"0.00","shipping":"0.00","total":"0.00"}).to_string();
        let ((), requests) = responses(
            &[
                (200, &data),
                (200, &quoted),
                (200, &data),
                (200, &data),
                (200, &single),
                (200, &data),
                (200, &data),
                (200, &empty),
                (200, &data),
            ],
            async {
                let (mut dom, captured) = mounted(
                    Screen::Cart(catalogue(), CHECKOUT.into()),
                    populated(),
                    TestDocument::new(vec![]),
                );
                assert!(dioxus_ssr::render(&dom).contains("Checking current prices"));
                let html = until(&mut dom, "Current total").await;
                assert!(html.contains("USD 24.60"));
                assert!(!html.contains("Refresh prices"));
                assert!(html.contains("for=\"cart-quantity-1\">Quantity</label>"));
                assert!(!html.contains("Quantity for"));
                assert!(!html.contains("Shipping"));
                assert!(!html.contains("Retry cart"));
                input(&mut dom, "cart-quantity-1", "input", "bad");
                dom.in_scope(ScopeId::ROOT, || {
                    assert_eq!(
                        captured
                            .get()
                            .expect("mounted cart context")
                            .cart
                            .peek()
                            .items[0]
                            .quantity,
                        2
                    );
                });
                input(&mut dom, "cart-quantity-1", "input", "1");
                assert!(!dioxus_ssr::render(&dom).contains("Current total"));
                assert!(
                    until(&mut dom, "Current total")
                        .await
                        .contains("1 × USD 12.30 = USD 12.30")
                );
                click(&mut dom, "aria-label", "Remove Shirt / Blue / Large");
                assert!(dioxus_ssr::render(&dom).contains("Your cart is empty"));
                until(&mut dom, "USD 0.00").await;
                assert_eq!(
                    dioxus_ssr::render(&dom)
                        .matches("checkout-payment-button")
                        .count(),
                    2
                );
            },
        )
        .await;
        assert_eq!(requests.len(), 9);
        assert!(requests[4].contains("\"quantity\":1"));
        assert!(requests[7].contains("\"items\":[]"));
    }

    #[tokio::test]
    async fn cart_line_shows_safe_current_picture_or_accessible_placeholder() {
        responses(&[], async {
            for url in [
                Some("https://images.test/blue.png"),
                Some("javascript:alert(1)"),
                None,
            ] {
                let mut data = catalogue();
                data.products[0].variants[0].image =
                    url.map(|url| domain::dto::CatalogueImage { url: url.into() });
                let (dom, _) = mounted(
                    Screen::Cart(data, CHECKOUT.into()),
                    populated(),
                    TestDocument::new(vec![]),
                );
                let html = dioxus_ssr::render(&dom);
                if url == Some("https://images.test/blue.png") {
                    assert!(html.contains("src=\"https://images.test/blue.png\""));
                    assert!(html.contains("alt=\"Shirt / Blue / Large\""));
                } else {
                    assert!(html.contains("No image available"));
                    assert!(!html.contains("javascript:"));
                }
            }
        })
        .await;
    }

    #[tokio::test]
    async fn changed_price_requires_acknowledgement_and_stock_reduction_unblocks_quote() {
        let mut changed = catalogue();
        changed.products[0].variants[0].final_price = "14.00".into();
        let data = serde_json::to_string(&changed).expect("serialize changed-price catalogue");
        let quoted = quote("14.00", 2);
        responses(
            &[(200, &data), (200, &data), (200, &quoted), (200, &data)],
            async {
                let (mut dom, _) = mounted(
                    Screen::Cart(catalogue(), CHECKOUT.into()),
                    populated(),
                    TestDocument::new(vec![]),
                );
                let html = until(&mut dom, "Accept updated prices").await;
                assert!(!html.contains("Current total"));
                assert!(html.contains("Acknowledgement required"));
                click(&mut dom, "__text", "Accept updated prices");
                assert!(until(&mut dom, "Current total").await.contains("USD 28.00"));
            },
        )
        .await;
        let mut stock = catalogue();
        stock.products[0].variants[0].max_quantity = 1;
        let data = serde_json::to_string(&stock).expect("serialize limited-stock catalogue");
        let quoted = quote("12.30", 1);
        responses(
            &[(200, &data), (200, &data), (200, &quoted), (200, &data)],
            async {
                let (mut dom, _) = mounted(
                    Screen::Cart(catalogue(), CHECKOUT.into()),
                    populated(),
                    TestDocument::new(vec![]),
                );
                let html = until(&mut dom, "Reduce to 1").await;
                assert!(!html.contains("Current total"));
                click(&mut dom, "__text", "Reduce to 1");
                until(&mut dom, "Current total").await;
            },
        )
        .await;
    }

    #[tokio::test]
    async fn missing_checkout_and_provider_failures_withhold_quotes() {
        let ((), requests) = responses(&[], async {
            let (mut dom, _) = mounted(
                Screen::Cart(catalogue(), "other".into()),
                populated(),
                TestDocument::new(vec![]),
            );
            let html = until(&mut dom, "Cart unavailable").await;
            assert!(html.contains("Open your current cart"));
            assert!(!html.contains("Current total"));
        })
        .await;
        assert_eq!(requests, Vec::<String>::new());
        let data = encoded();
        let quoted = quote("12.30", 2);
        responses(
            &[
                (502, r#"{"error":"Current catalogue unavailable"}"#),
                (200, &data),
                (200, &quoted),
                (200, &data),
            ],
            async {
                let (mut dom, _) = mounted(
                    Screen::Cart(catalogue(), CHECKOUT.into()),
                    populated(),
                    TestDocument::new(vec![
                        serde_json::json!({"ok":true,"value":null}),
                        serde_json::json!(true),
                    ]),
                );
                let html = until(&mut dom, "Current total").await;
                assert!(!html.contains("Current catalogue unavailable"));
                assert!(!html.contains("Retry cart"));
            },
        )
        .await;
        let data = encoded();
        responses(
            &[
                (200, &data),
                (503, r#"{"error":"Quote unavailable"}"#),
                (200, &data),
                (200, &quoted),
                (200, &data),
            ],
            async {
                let (mut dom, _) = mounted(
                    Screen::Cart(catalogue(), CHECKOUT.into()),
                    populated(),
                    TestDocument::new(vec![
                        serde_json::json!({"ok":true,"value":null}),
                        serde_json::json!(true),
                    ]),
                );
                let html = until(&mut dom, "Current total").await;
                assert!(!html.contains("Quote unavailable"));
                assert!(!html.contains("Retry cart"));
            },
        )
        .await;
    }

    #[tokio::test]
    async fn quote_recovery_rechecks_prices_without_accepting_changes_for_the_customer() {
        let data = encoded();
        let mut changed = catalogue();
        changed.products[0].variants[0].final_price = "14.00".into();
        let changed = serde_json::to_string(&changed).expect("changed catalogue");
        let document = TestDocument::new(vec![
            serde_json::json!({"ok":true,"value":null}),
            serde_json::json!(true),
        ]);
        let ((), requests) = responses(
            &[
                (200, &data),
                (503, r#"{"error":"Quote unavailable"}"#),
                (200, &changed),
            ],
            async {
                let (mut dom, captured) = mounted(
                    Screen::Cart(catalogue(), CHECKOUT.into()),
                    populated(),
                    document.clone(),
                );
                let html = until(&mut dom, "Accept updated prices").await;
                assert!(!html.contains("Current total"));
                assert!(!html.contains("Retry cart"));
                assert!(html.contains("Acknowledgement required"));
                dom.in_scope(ScopeId::ROOT, || {
                    let context = captured.get().expect("cart context");
                    assert_eq!(*context.cart.peek(), populated());
                });
            },
        )
        .await;
        assert_eq!(requests.len(), 3);
        assert!(requests[1].starts_with("POST /v1/conventions/fair-a/quote"));
        assert!(requests[2].starts_with("GET /v1/conventions/fair-a/products"));
        assert_eq!(
            document
                .scripts
                .borrow()
                .iter()
                .filter(|script| script.contains("setTimeout"))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn changing_cart_cancels_the_waiting_quote_and_only_new_quantities_are_payable() {
        let data = encoded();
        let single = quote("12.30", 1);
        let document = TestDocument::new(vec![
            serde_json::json!({"ok":true,"value":null}),
            serde_json::json!({"__eval_pending":true}),
        ]);
        let ((), requests) = responses(
            &[
                (200, &data),
                (503, r#"{"error":"Quote unavailable"}"#),
                (200, &data),
                (200, &single),
                (200, &data),
            ],
            async {
                let (mut dom, _) = mounted(
                    Screen::Cart(catalogue(), CHECKOUT.into()),
                    populated(),
                    document.clone(),
                );
                for _ in 0..200 {
                    let _ = dom.wait_for_work().now_or_never();
                    dom.render_immediate_to_vec();
                    if document
                        .scripts
                        .borrow()
                        .iter()
                        .any(|script| script.contains("setTimeout"))
                    {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
                let html = dioxus_ssr::render(&dom);
                assert!(html.contains("Checking current prices"));
                assert!(!html.contains("Current total"));
                assert!(!html.contains("Retry cart"));
                input(&mut dom, "cart-quantity-1", "input", "1");
                let html = until(&mut dom, "Current total").await;
                assert!(html.contains("1 × USD 12.30 = USD 12.30"));
                assert!(!html.contains("USD 24.60"));
                drop(dom);
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            },
        )
        .await;
        assert_eq!(requests.len(), 5);
        assert!(requests[1].contains("\"quantity\":2"));
        assert!(requests[3].contains("\"quantity\":1"));
        assert_eq!(
            document
                .scripts
                .borrow()
                .iter()
                .filter(|script| script.contains("setTimeout"))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn replaced_convention_cannot_reuse_the_old_cart_during_initial_or_post_quote_reads() {
        let mut replacement = catalogue();
        replacement.convention.id = "replacement".into();
        replacement.convention.currency_code = "THB".into();
        let replaced = serde_json::to_string(&replacement)
            .expect("serialize replacement convention catalogue");
        let data = encoded();
        let quoted = quote("12.30", 2);
        for replies in [
            vec![(200, replaced.as_str())],
            vec![
                (200, data.as_str()),
                (200, quoted.as_str()),
                (200, replaced.as_str()),
            ],
            vec![
                (200, data.as_str()),
                (409, r#"{"error":"Catalogue changed"}"#),
                (200, replaced.as_str()),
            ],
        ] {
            responses(&replies, async {
                let (mut dom, _) = mounted(
                    Screen::Cart(catalogue(), CHECKOUT.into()),
                    populated(),
                    TestDocument::new(vec![]),
                );
                let html = until(&mut dom, "This convention has changed").await;
                assert!(!html.contains("Current total"));
                assert!(!html.contains("THB"));
            })
            .await;
        }
        let invalid = quoted.replace("USD", "THB");
        responses(&[(200, &data), (200, &invalid), (200, &data)], async {
            let (mut dom, _) = mounted(
                Screen::Cart(catalogue(), CHECKOUT.into()),
                populated(),
                TestDocument::new(vec![]),
            );
            let html = until(&mut dom, "invalid cart quote").await;
            assert!(!html.contains("Current total"));
        })
        .await;
    }
}

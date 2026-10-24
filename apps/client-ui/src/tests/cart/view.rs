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
    assert!(html.contains("USD 0.00"));
    assert!(!html.contains("button"));
}

#[cfg(not(target_arch = "wasm32"))]
mod mounted {
    use super::*;
    use crate::{
        api::tests::{catalogue, responses},
        pages::tests::{CHECKOUT, Screen, TestDocument, click, empty_cart, input, mounted, until},
    };

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
                assert!(until(&mut dom, "Current total").await.contains("USD 24.60"));
                input(&mut dom, "cart-quantity-1", "change", "bad");
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
                input(&mut dom, "cart-quantity-1", "change", "1");
                assert!(!dioxus_ssr::render(&dom).contains("Current total"));
                assert!(
                    until(&mut dom, "Current total")
                        .await
                        .contains("1 × USD 12.30 = USD 12.30")
                );
                click(&mut dom, "aria-label", "Remove Shirt / Blue / Large");
                assert!(dioxus_ssr::render(&dom).contains("Your cart is empty"));
                until(&mut dom, "USD 0.00").await;
                assert!(!dioxus_ssr::render(&dom).contains("payment-button"));
            },
        )
        .await;
        assert_eq!(requests.len(), 9);
        assert!(requests[4].contains("\"quantity\":1"));
        assert!(requests[7].contains("\"items\":[]"));
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
                    TestDocument::new(vec![]),
                );
                let html = until(&mut dom, "Current catalogue unavailable").await;
                assert!(!html.contains("Current total"));
                click(&mut dom, "__text", "Refresh prices and availability");
                until(&mut dom, "Current total").await;
            },
        )
        .await;
        let data = encoded();
        responses(
            &[(200, &data), (503, r#"{"error":"Quote unavailable"}"#)],
            async {
                let (mut dom, _) = mounted(
                    Screen::Cart(catalogue(), CHECKOUT.into()),
                    populated(),
                    TestDocument::new(vec![]),
                );
                let html = until(&mut dom, "Quote unavailable").await;
                assert!(!html.contains("Current total"));
            },
        )
        .await;
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

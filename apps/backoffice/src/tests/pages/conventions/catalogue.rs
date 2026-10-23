use super::*;
#[test]
fn provider_images_are_safe_https_urls_only() {
    assert!(safe_image_url("https://cdn.example.test/image.png"));
    for value in [
        "javascript:alert(1)",
        "data:image/svg+xml,<svg>",
        "//cdn.example.test/image",
        "https://user:password@cdn.example.test/a",
    ] {
        assert!(!safe_image_url(value));
    }
}

#[test]
fn catalogue_submission_preserves_numeric_location_and_omits_empty_search() {
    let query = BrowseQuery {
        location: "123".into(),
        ..Default::default()
    }
    .to_api_query();
    assert_eq!(query.location_id, "123");
    assert_eq!(query.page_size, Some(25));
    assert_eq!(query.search, None);
    assert_eq!(query.cursor, None);
}

#[test]
fn catalogue_submission_preserves_nonempty_search_and_pagination_cursor() {
    let query = BrowseQuery {
        location: "1".into(),
        search: " title:Fox AND tag:'a&b' ".into(),
        cursor: Some("abc+/=".into()),
    }
    .to_api_query();
    assert_eq!(query.location_id, "1");
    assert_eq!(query.search.as_deref(), Some(" title:Fox AND tag:'a&b' "));
    assert_eq!(query.cursor.as_deref(), Some("abc+/="));
}

fn catalogue_fixture(variant_image: Option<&str>) -> CatalogueResponse {
    use domain::dto::*;
    CatalogueResponse {
        products: vec![CatalogueProduct {
            id: "1".into(),
            title: "Fox shirt".into(),
            description: "Cotton".into(),
            featured_image: Some(CatalogueImage {
                url: "https://cdn.test/product.png".into(),
            }),
            variants: vec![CatalogueVariant {
                id: "2".into(),
                inventory_item_id: "3".into(),
                title: "Blue / M".into(),
                sku: Some("FOX-B-M".into()),
                image: variant_image.map(|url| CatalogueImage { url: url.into() }),
                selected_options: vec![],
                price: CataloguePrice {
                    amount: "18.50".into(),
                    currency_code: "THB".into(),
                },
                tracked: true,
                inventory_policy: CatalogueInventoryPolicy::Deny,
                available_quantity: Some(0),
                eligible: true,
            }],
        }],
        page_info: PageInfo {
            has_next_page: false,
            end_cursor: None,
        },
    }
}
#[test]
fn catalogue_retains_sku_and_uses_only_the_actual_safe_variant_image() {
    let page = BrowsePage::from_response(catalogue_fixture(Some("https://cdn.test/blue.png")));
    assert_eq!(page.items[0].sku.as_deref(), Some("FOX-B-M"));
    assert_eq!(
        page.items[0].image.as_ref().map(|image| image.0.as_str()),
        Some("https://cdn.test/blue.png")
    );
    assert_eq!(
        page.items[0].image.as_ref().map(|image| image.1.as_str()),
        Some("Blue / M")
    );
    for image in [None, Some("javascript:alert(1)")] {
        let page = BrowsePage::from_response(catalogue_fixture(image));
        assert!(
            page.items[0].image.is_none(),
            "never substitute the product image"
        );
    }
    assert_eq!(
        detail_text(Some("  "), "SKU unavailable"),
        "SKU unavailable"
    );
}

#[test]
fn catalogue_image_label_falls_back_without_a_variant_title_and_preserves_selection_guard() {
    let mut response = catalogue_fixture(Some("https://cdn.test/blue.png"));
    response.products[0].variants[0].title = "  ".into();
    response.products[0].variants[0].eligible = false;
    let page = BrowsePage::from_response(response);
    assert_eq!(
        page.items[0].image.as_ref().map(|image| image.1.as_str()),
        Some("Variant image")
    );
    assert!(!page.items[0].eligible);
}

#[cfg(not(target_arch = "wasm32"))]
mod mounted {
    use super::*;
    use crate::test_events::{FormEventConverter, form_event, listeners, render_changes};
    use dioxus::dioxus_core::Mutations;

    const ID: &str = "da41d46b-f1ca-4a76-a774-62c269d28731";
    fn editor() -> Element {
        rsx! { SelectionEditor { id: ID, currency: "THB", enabled: false, on_dirty: move |_| {} } }
    }
    fn mount() -> VirtualDom {
        dioxus::html::set_event_converter(Box::new(FormEventConverter));
        let settings = consume_context::<crate::config::Settings>();
        let dom = VirtualDom::new(editor).with_root_context(settings);
        dom.in_scope(ScopeId::ROOT, || {
            *session::SESSION.write() = Some(session::Session {
                admin_id: 1,
                username: "fixture".into(),
                role: domain::AdminRole::Admin,
                expires_at: u64::MAX,
            });
        });
        dom
    }
    async fn settle(dom: &mut VirtualDom) -> Mutations {
        let mut changes = render_changes(dom);
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            changes.edits.extend(render_changes(dom).edits);
        }
        changes
    }
    fn saved(extra: &str) -> String {
        format!(
            r#"{{"id":"row","convention_id":"{ID}","shopify_product_id":"1","shopify_variant_id":"2","final_price":"12.990","created_at":"now","updated_at":"now"{extra}}}"#
        )
    }
    fn page(row: &str) -> String {
        format!(r#"{{"items":[{row}],"page":1,"per_page":100,"has_next":false}}"#)
    }
    #[tokio::test]
    async fn current_saved_selection_renders_persisted_identity_image_and_exact_price_on_new_mount()
    {
        let body = page(&saved(
            r#", "inventory_policy":"DENY", "product_title":"Fox shirt", "variant_title":"Blue / M", "sku":"FOX-B-M", "product_image":null, "variant_image":{"url":"https://cdn.test/blue.png"}"#,
        ));
        let ((), requests) = crate::api::tests::metadata_responses(&[(200, &body)], async {
            let mut dom = mount();
            dom.rebuild_in_place();
            settle(&mut dom).await;
            let html = dioxus_ssr::render(&dom);
            for expected in [
                "Fox shirt",
                "Blue / M",
                "FOX-B-M",
                "https://cdn.test/blue.png",
                r#"alt="Blue / M""#,
                "12.990",
            ] {
                assert!(html.contains(expected), "missing {expected}: {html}");
            }
        })
        .await;
        assert_eq!(requests.len(), 1, "display must not fetch each variant");
    }
    const CATALOGUE: &str = r#"{"products":[{"id":"1","title":"Fox shirt","description":"Cotton","featured_image":{"url":"https://cdn.test/product.png"},"variants":[{"id":"2","inventory_item_id":"3","title":"Blue / M","sku":"FOX-B-M","image":{"url":"https://cdn.test/blue.png"},"selected_options":[],"price":{"amount":"18.50","currency_code":"THB"},"tracked":true,"inventory_policy":"DENY","available_quantity":0,"eligible":true}]}],"page_info":{"has_next_page":true,"end_cursor":"next"}}"#;
    const EMPTY_PAGE: &str =
        r#"{"products":[],"page_info":{"has_next_page":false,"end_cursor":null}}"#;
    const EMPTY_SAVED: &str = r#"{"items":[],"page":1,"per_page":100,"has_next":false}"#;
    fn save_disabled(dom: &VirtualDom) -> bool {
        dioxus_ssr::render(dom)
            .split("Save selected variants and prices")
            .next()
            .expect("valid fixture value")
            .rsplit("<button")
            .next()
            .expect("valid fixture value")
            .contains("disabled")
    }
    #[tokio::test]
    async fn catalogue_select_override_save_and_paging_preserve_details_with_legacy_response_and_failure()
     {
        let response = format!(r#"{{"items":[{}]}}"#, saved(""));
        let ((), requests) = crate::api::tests::metadata_responses(
            &[
                (200, EMPTY_SAVED),
                (200, CATALOGUE),
                (200, EMPTY_PAGE),
                (502, r#"{"error":"Provider unavailable"}"#),
                (200, &response),
            ],
            async {
                let mut dom = mount();
                let initial = dom.rebuild_to_vec();
                settle(&mut dom).await;
                assert!(save_disabled(&dom));
                let inputs = listeners(&initial, "input");
                form_event(
                    &dom,
                    crate::test_events::element_id(&initial, "browse-shopify-catalogue"),
                    "click",
                    "",
                );
                form_event(&dom, inputs[0], "input", "Shirts");
                form_event(&dom, inputs[1], "input", "Fox");
                form_event(&dom, listeners(&initial, "submit")[0], "submit", "");
                let browse = settle(&mut dom).await;
                let html = dioxus_ssr::render(&dom);
                for expected in [
                    "Fox shirt",
                    "Blue / M",
                    "FOX-B-M",
                    "https://cdn.test/blue.png",
                ] {
                    assert!(html.contains(expected), "missing {expected}: {html}");
                }
                assert!(html.contains("https://cdn.test/product.png"));
                assert!(html.contains("convention-thumbnail"));
                let clicks = listeners(&browse, "click");
                form_event(&dom, clicks[0], "click", "");
                let selected = settle(&mut dom).await;
                form_event(
                    &dom,
                    crate::test_events::element_id(&selected, "variant-price-2"),
                    "input",
                    "12.990",
                );
                settle(&mut dom).await;
                form_event(&dom, inputs[0], "input", "Changed draft type");
                form_event(&dom, inputs[1], "input", "Changed draft title");
                // Close and reopen the same mounted picker; selection, query and results survive.
                let initial_clicks = listeners(&initial, "click");
                form_event(
                    &dom,
                    *initial_clicks.last().expect("popup close"),
                    "click",
                    "",
                );
                settle(&mut dom).await;
                form_event(
                    &dom,
                    crate::test_events::element_id(&initial, "browse-shopify-catalogue"),
                    "click",
                    "",
                );
                settle(&mut dom).await;
                let reopened = dioxus_ssr::render(&dom);
                assert!(reopened.contains("Already selected"));
                assert!(reopened.contains("Changed draft title"));
                assert!(reopened.contains("12.990"));
                form_event(&dom, clicks[1], "click", "");
                settle(&mut dom).await;
                let html = dioxus_ssr::render(&dom);
                assert!(html.contains("No variants match"));
                for expected in [
                    "Fox shirt",
                    "Blue / M",
                    "FOX-B-M",
                    "https://cdn.test/blue.png",
                    "12.990",
                ] {
                    assert!(
                        html.contains(expected),
                        "selected details lost on paging: {html}"
                    );
                }
                assert!(!save_disabled(&dom));
                assert!(html.contains("Source price: 18.50 THB"));
                assert!(!html.contains("Available stock"));
                let save = crate::test_events::element_id(&initial, "save-convention-variants");
                form_event(&dom, save, "click", "");
                settle(&mut dom).await;
                assert!(dioxus_ssr::render(&dom).contains("Provider unavailable"));
                assert!(!save_disabled(&dom));
                form_event(&dom, save, "click", "");
                settle(&mut dom).await;
                let html = dioxus_ssr::render(&dom);
                assert!(html.contains("Selected variants and prices saved"));
                for expected in [
                    "Fox shirt",
                    "Blue / M",
                    "FOX-B-M",
                    "https://cdn.test/blue.png",
                    "12.990",
                ] {
                    assert!(
                        html.contains(expected),
                        "legacy PUT lost selected metadata: {html}"
                    );
                }
                assert!(save_disabled(&dom));
            },
        )
        .await;
        assert_eq!(requests.len(), 5, "no per-row requests");
        assert!(requests[1].contains("location_id=65761018027"));
        assert!(requests[2].contains("cursor=next"));
        for request in &requests[1..3] {
            let target = request.split_whitespace().nth(1).expect("request target");
            let url = reqwest::Url::parse(&format!("http://test{target}")).expect("URL");
            let search = url
                .query_pairs()
                .find(|(key, _)| key == "search")
                .expect("search")
                .1
                .into_owned();
            assert_eq!(search, r#"product_type:"Shirts" AND title:"Fox""#);
        }
        for request in &requests[3..] {
            assert!(request.ends_with(r#"{"items":[{"shopify_product_id":"1","shopify_variant_id":"2","final_price":"12.990"}]}"#));
        }
    }
    #[tokio::test]
    async fn enriched_put_explicit_null_removes_stale_details_without_dirtying_price() {
        let initial = page(&saved(
            r#", "product_title":"Fox shirt", "variant_title":"Blue / M", "sku":"OLD-SKU", "variant_image":{"url":"https://cdn.test/old.png"}, "available_quantity":5"#,
        ));
        let response = format!(
            r#"{{"items":[{}]}}"#,
            saved(r#", "sku":null, "variant_image":null, "available_quantity":null"#)
        );
        let ((), requests) =
            crate::api::tests::metadata_responses(&[(200, &initial), (200, &response)], async {
                let mut dom = mount();
                let initial = dom.rebuild_to_vec();
                let loaded = settle(&mut dom).await;
                assert!(save_disabled(&dom));
                form_event(
                    &dom,
                    crate::test_events::element_id(&loaded, "variant-price-2"),
                    "input",
                    "19.990",
                );
                settle(&mut dom).await;
                form_event(
                    &dom,
                    crate::test_events::element_id(&initial, "save-convention-variants"),
                    "click",
                    "",
                );
                settle(&mut dom).await;
                let html = dioxus_ssr::render(&dom);
                for expected in ["SKU unavailable", "No variant image", "Blue / M", "12.990"] {
                    assert!(html.contains(expected), "{html}");
                }
                assert!(!html.contains("OLD-SKU"));
                assert!(!html.contains("https://cdn.test/old.png"));
                assert!(!html.contains("Last loaded at location"));
                assert!(save_disabled(&dom));
            })
            .await;
        assert_eq!(requests.len(), 2);
    }
    #[tokio::test]
    async fn fresh_legacy_load_has_honest_placeholders_and_no_false_dirty_state() {
        let body = page(&saved(""));
        crate::api::tests::metadata_responses(&[(200, &body)], async {
            let mut dom = mount();
            dom.rebuild_in_place();
            settle(&mut dom).await;
            let html = dioxus_ssr::render(&dom);
            for expected in [
                "Variant title unavailable",
                "SKU unavailable",
                "No variant image",
                "12.990",
            ] {
                assert!(html.contains(expected), "{html}");
            }
            assert!(save_disabled(&dom));
        })
        .await;
    }
}

#[test]
fn generated_search_intersects_quoted_trimmed_fields_and_omits_blanks() {
    assert_eq!(
        catalogue_search("  Shirts  ", " Fox ").unwrap(),
        r#"product_type:"Shirts" AND title:"Fox""#
    );
    assert_eq!(catalogue_search("  ", " Fox ").unwrap(), r#"title:"Fox""#);
    assert_eq!(
        catalogue_search("Shirts", "").unwrap(),
        r#"product_type:"Shirts""#
    );
    assert_eq!(catalogue_search(" ", "\n").unwrap(), "");
    assert_eq!(
        catalogue_search(r#"x:" OR title:(bad) \"#, "").unwrap(),
        r#"product_type:"x\:\" OR title\:\(bad\) \\""#
    );
}

#[test]
fn generated_search_limit_counts_escaped_utf8_bytes() {
    assert!(catalogue_search("", &"a".repeat(2040)).is_ok());
    assert!(catalogue_search("", &"a".repeat(2041)).is_err());
    assert!(catalogue_search("", &":".repeat(1021)).is_err());
    assert!(catalogue_search("", &"ก".repeat(681)).is_err());
}

#[test]
fn product_and_variant_images_are_distinct_and_independently_safe() {
    let mut response = catalogue_fixture(Some("https://cdn.test/blue.png"));
    response.products[0].featured_image.as_mut().unwrap().url = "javascript:alert(1)".into();
    let page = BrowsePage::from_response(response);
    assert!(page.items[0].product_image.is_none());
    assert!(page.items[0].image.is_some());
    let page = BrowsePage::from_response(catalogue_fixture(None));
    assert!(page.items[0].image.is_none());
    assert_eq!(
        page.items[0].product_image.as_ref().unwrap().0,
        "https://cdn.test/product.png"
    );
}

#[test]
fn display_title_combines_sku_product_and_variant() {
    fn app() -> Element {
        rsx! { VariantTitle { sku: Some("XXXKEYNOS01".into()), product: Some("Dangle game series keychain".into()), variant: Some("Umbrella kirby".into()) } }
    }
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("XXXKEYNOS01"));
    assert!(html.contains("Dangle game series keychain"));
    assert!(html.contains("<strong>"));
    assert!(html.contains("Umbrella kirby"));
}

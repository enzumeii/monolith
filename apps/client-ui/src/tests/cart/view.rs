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

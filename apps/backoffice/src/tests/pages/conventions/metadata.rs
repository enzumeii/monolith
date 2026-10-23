use super::*;

#[test]
fn metadata_fields_keep_identity_readonly_and_date_inputs_are_calendar_dates() {
    fn app() -> Element {
        let value = Metadata {
            slug: "event".into(),
            currency: "THB".into(),
            methods: vec![ConventionPaymentMethod::Stripe],
            ..Default::default()
        };
        let draft = use_signal(|| value);
        rsx! { MetadataFields { draft, options: vec![("THB".into(), vec![ConventionPaymentMethod::Stripe])], editing: true } }
    }
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(html.contains("readonly"));
    assert!(
        html.contains("<option value=\"THB\" selected=true>THB</option>"),
        "{html}"
    );
    assert!(html.contains("<option value=\"\">Choose currency</option>"));
    assert!(html.contains("type=\"date\""));
    assert!(html.contains("Enable preorders"));
    assert!(html.contains("Description preview"));
    assert!(!html.contains("datetime-local"));
}
#[test]
fn unsupported_saved_capabilities_are_preserved_and_explained() {
    fn app() -> Element {
        let value = Metadata {
            currency: "JPY".into(),
            methods: vec![ConventionPaymentMethod::Paypal],
            ..Default::default()
        };
        let draft = use_signal(|| value);
        rsx! { MetadataFields { draft, options: Vec::new(), editing: true } }
    }
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(
        html.contains("<option value=\"JPY\" selected=true>JPY (currently unsupported)</option>"),
        "{html}"
    );
    assert!(html.contains("<option value=\"\">Choose currency</option>"));
    assert!(html.contains("PayPal"));
    assert!(html.contains("stored method no longer configured"));
    assert!(html.contains("You can edit metadata or disable preorders"));
    let methods = html
        .split("Payment methods")
        .nth(1)
        .expect("payment fieldset")
        .split("</fieldset>")
        .next()
        .expect("fieldset body");
    assert!(
        !methods.contains("disabled"),
        "stored unsupported method must remain removable"
    );
}

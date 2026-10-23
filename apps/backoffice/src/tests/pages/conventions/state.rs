use super::*;
use domain::dto::ConventionVariantDTO;

#[test]
fn exact_prices_remain_strings_and_search_never_discards_selected_items() {
    let mut selection = Selection::default();
    selection.insert("1".into(), "2".into());
    selection
        .items
        .get_mut("2")
        .expect("valid fixture value")
        .final_price = "12.990".into();
    assert!(
        selection.request().is_err(),
        "incomplete load cannot replace saved selections"
    );
    selection.loaded = true;
    let request = selection.request().expect("valid fixture value");
    assert_eq!(request.items[0].final_price, "12.990");
    assert!(positive_decimal("0.001"));
    for value in ["0", "0.00", "-1", "1e2", "NaN", "1.2.3", ".5", "1."] {
        assert!(!positive_decimal(value), "{value}");
    }
}
#[test]
fn loading_saved_memberships_is_atomic_and_sets_the_persisted_baseline() {
    let item = ConventionVariantDTO {
        id: "row".into(),
        convention_id: "con".into(),
        shopify_product_id: "11".into(),
        shopify_variant_id: "22".into(),
        final_price: "12.99".into(),
        created_at: String::new(),
        updated_at: String::new(),
    };
    let selection = Selection::from_saved(&[item]);
    assert!(selection.loaded);
    assert_eq!(
        selection.request().expect("valid fixture value").items[0].final_price,
        "12.99"
    );
}
#[test]
fn metadata_patch_omits_logo_and_immutable_fields() {
    let mut form = Metadata {
        slug: "event".into(),
        name: "Event".into(),
        country: "TH".into(),
        start: "2026-11-01".into(),
        end: "2026-11-02".into(),
        currency: "THB".into(),
        markdown: "Description".into(),
        methods: vec![ConventionPaymentMethod::Stripe],
        ..Default::default()
    };
    let saved = form.clone();
    form.name = "Changed".into();
    let patch = form.patch(&saved).expect("changed metadata");
    assert!(matches!(patch.logo_location, PatchField::Missing));
    assert!(form.create().is_ok());
    for slug in [
        "../event",
        "event/other",
        "Event",
        "api",
        "event--two",
        "-event",
        "event-",
    ] {
        form.slug = slug.into();
        assert!(form.create().is_err(), "{slug}");
    }
    form.slug = "event".into();
    form.end = "2026-10-31".into();
    assert!(form.create().is_err());
}

#[test]
fn replacement_enforces_the_membership_limit_and_preserves_exact_prices() {
    let mut selection = Selection {
        loaded: true,
        ..Default::default()
    };
    for index in 0..251 {
        selection.insert("product".into(), index.to_string());
        selection
            .items
            .get_mut(&index.to_string())
            .expect("valid fixture value")
            .final_price = "0.001".into();
    }
    assert!(selection.request().is_err());
    selection.items.remove("250");
    let request = selection.request().expect("valid fixture value");
    assert_eq!(request.items.len(), 250);
    assert!(request.items.iter().all(|item| item.final_price == "0.001"));
}

#[test]
fn metadata_patch_only_emits_changed_fields_and_noop_cannot_save() {
    let saved = Metadata {
        slug: "event".into(),
        name: "Event".into(),
        country: "TH".into(),
        start: "2026-11-01".into(),
        end: "2026-11-02".into(),
        currency: "REMOVED".into(),
        markdown: "Description".into(),
        methods: vec![ConventionPaymentMethod::Paypal],
        enabled: true,
    };
    assert!(saved.patch(&saved).is_err());
    let mut draft = saved.clone();
    draft.name = "Changed".into();
    let patch = draft.patch(&saved).expect("name-only update");
    assert_eq!(patch.name, PatchField::Set("Changed".into()));
    assert!(patch.payment_methods.is_missing());
    assert!(patch.preorder_enabled.is_missing());
    assert!(patch.logo_location.is_missing());
    assert!(!draft.requires_capabilities(&saved));
    draft = saved.clone();
    draft.enabled = false;
    let patch = draft
        .patch(&saved)
        .expect("disable unsupported saved currency");
    assert_eq!(patch.preorder_enabled, PatchField::Set(false));
    assert!(patch.payment_methods.is_missing());
    assert!(!draft.requires_capabilities(&saved));
    let disabled = draft.clone();
    draft.enabled = true;
    assert!(draft.requires_capabilities(&disabled));
    draft = saved.clone();
    draft.methods = vec![ConventionPaymentMethod::Stripe];
    assert!(draft.requires_capabilities(&saved));
}

#[cfg(not(target_arch = "wasm32"))]
#[tokio::test]
async fn name_only_edit_sends_no_capability_or_enable_fields_on_the_wire() {
    let saved = Metadata {
        slug: "event".into(),
        name: "Event".into(),
        country: "TH".into(),
        start: "2026-11-01".into(),
        end: "2026-11-02".into(),
        currency: "JPY".into(),
        markdown: "Description".into(),
        methods: vec![ConventionPaymentMethod::Paypal],
        enabled: true,
    };
    let mut draft = saved.clone();
    draft.name = "Changed".into();
    let patch = draft.patch(&saved).expect("name-only patch");
    let (_, requests) = crate::api::tests::metadata_responses(
        &[(500, r#"{"error":"controlled failure"}"#)],
        crate::api::conventions::update("da41d46b-f1ca-4a76-a774-62c269d28731", patch),
    )
    .await;
    let request = &requests[0];
    assert!(request.ends_with(r#"{"name":"Changed"}"#));
    assert!(!request.contains("payment_methods"));
    assert!(!request.contains("preorder_enabled"));
}

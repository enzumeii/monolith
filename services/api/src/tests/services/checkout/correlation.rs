#![allow(clippy::unwrap_used)]
use super::*;
use crate::services::checkout::tests::checkout;

fn row() -> Checkout {
    let mut row = checkout();
    row.correlation_version = METADATA_VERSION;
    row.convention_slug = Some("qa-checkout-59b5fcc9".into());
    row
}

#[test]
fn identity_roundtrip_requires_exact_canonical_singletons() {
    let row = row();
    let attributes = identity_attributes(&row).unwrap();
    assert_eq!(attributes.len(), 5);
    assert_eq!(
        parse_identity(&attributes).unwrap(),
        Some(Identity::from_checkout(&row).unwrap())
    );
    assert!(identity_matches(&row, &attributes));
    for key in [
        VERSION_KEY,
        CHECKOUT_KEY,
        CONVENTION_KEY,
        SLUG_KEY,
        PROVIDER_KEY,
    ] {
        let mut missing = attributes.clone();
        missing.retain(|attribute| attribute.key != key);
        assert!(parse_identity(&missing).is_err(), "missing {key}");
        let mut duplicate = attributes.clone();
        duplicate.push(
            attributes
                .iter()
                .find(|attribute| attribute.key == key)
                .unwrap()
                .clone(),
        );
        assert!(parse_identity(&duplicate).is_err(), "duplicate {key}");
        for invalid in ["", " ", "bad\nvalue"] {
            let mut changed = attributes.clone();
            changed
                .iter_mut()
                .find(|attribute| attribute.key == key)
                .unwrap()
                .value = invalid.into();
            assert!(parse_identity(&changed).is_err(), "invalid {key}");
        }
    }
}

#[test]
fn identity_rejects_conflicts_version_noncanonical_ids_slug_provider_and_opposite_refs() {
    let row = row();
    let attributes = identity_attributes(&row).unwrap();
    for (key, value) in [
        (VERSION_KEY, "1".into()),
        (VERSION_KEY, "02".into()),
        (CHECKOUT_KEY, row.id.simple().to_string()),
        (
            CONVENTION_KEY,
            "AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA".into(),
        ),
        (SLUG_KEY, "bad--slug".into()),
        (SLUG_KEY, "A".into()),
        (PROVIDER_KEY, "Stripe".into()),
    ] {
        let mut changed = attributes.clone();
        changed
            .iter_mut()
            .find(|attribute| attribute.key == key)
            .unwrap()
            .value = value;
        assert!(parse_identity(&changed).is_err(), "invalid {key}");
    }
    for key in [PAYPAL_ORDER_KEY, PAYPAL_CAPTURE_KEY] {
        let mut changed = attributes.clone();
        changed.push(CustomAttribute {
            key: key.into(),
            value: "paypal-id".into(),
        });
        assert!(parse_identity(&changed).is_err());
    }
    for key in [STRIPE_SESSION_KEY, STRIPE_INTENT_KEY, STRIPE_CHARGE_KEY] {
        let mut changed = attributes.clone();
        changed.push(CustomAttribute {
            key: key.into(),
            value: "stripe-id".into(),
        });
        assert!(parse_identity(&changed).is_ok());
        changed.push(CustomAttribute {
            key: key.into(),
            value: "stripe-id".into(),
        });
        assert!(parse_identity(&changed).is_err());
    }
    for value in [
        "space inside",
        "colon:value",
        "comma,value",
        "too-long".repeat(40).as_str(),
    ] {
        let mut changed = attributes.clone();
        changed.push(CustomAttribute {
            key: STRIPE_SESSION_KEY.into(),
            value: value.into(),
        });
        assert!(parse_identity(&changed).is_err());
    }
    let mut foreign = row.clone();
    foreign.id = Uuid::new_v4();
    assert!(!identity_matches(&foreign, &attributes));
    foreign = row.clone();
    foreign.convention_id = Uuid::new_v4();
    assert!(!identity_matches(&foreign, &attributes));
    foreign = row.clone();
    foreign.convention_slug = Some("other-slug".into());
    assert!(!identity_matches(&foreign, &attributes));
}

#[test]
fn absent_metadata_preserves_legacy_and_unrelated_attributes() {
    assert_eq!(parse_identity(&[]).unwrap(), None);
    let attrs = vec![
        CustomAttribute {
            key: SLUG_KEY.into(),
            value: "legacy-slug".into(),
        },
        CustomAttribute {
            key: "merchant_note".into(),
            value: "keep".into(),
        },
    ];
    assert_eq!(parse_identity(&attrs).unwrap(), None);
    let row = row();
    let mut current = identity_attributes(&row).unwrap();
    current.extend(attrs.into_iter().filter(|attr| attr.key != SLUG_KEY));
    assert!(identity_matches(&row, &current));
    assert!(parse_identity(&current).is_ok());
}

#[test]
fn paypal_identity_rejects_stripe_references() {
    let mut row = row();
    row.provider = "paypal".into();
    let attributes = identity_attributes(&row).unwrap();
    assert!(identity_matches(&row, &attributes));
    for key in [STRIPE_SESSION_KEY, STRIPE_INTENT_KEY, STRIPE_CHARGE_KEY] {
        let mut changed = attributes.clone();
        changed.push(CustomAttribute {
            key: key.into(),
            value: "reference".into(),
        });
        assert!(parse_identity(&changed).is_err());
    }
}

#[test]
fn legacy_identity_labels_cannot_contradict_metadata_but_operator_labels_are_allowed() {
    let identity = Identity::from_checkout(&row()).unwrap();
    assert!(
        validate_legacy_identity(
            &identity,
            &[
                "operator-note".into(),
                format!("co:{}", identity.checkout_id),
                format!("cv:{}", identity.convention_id)
            ]
        )
        .is_ok()
    );
    for tags in [
        vec!["co:old-clutter".into()],
        vec![format!("co:{}", Uuid::new_v4())],
        vec![format!("cv:{}", Uuid::new_v4())],
        vec![format!("co:{}", identity.checkout_id); 2],
    ] {
        assert!(validate_legacy_identity(&identity, &tags).is_err());
    }
}

#[test]
fn independent_money_metadata_roundtrips_and_rejects_partial_or_duplicate_snapshots() {
    let mut row = row();
    row.correlation_version = INDEPENDENT_VERSION;
    row.payment_amount_minor = Some(12000);
    row.payment_currency = Some("THB".into());
    row.shopify_amount_minor = Some(400);
    row.shopify_currency = Some("AUD".into());
    let attrs = identity_attributes(&row).unwrap();
    let snapshot = parse_money_snapshots(&attrs).unwrap().unwrap();
    assert_eq!(snapshot.payment.minor, 12000);
    assert_eq!(snapshot.payment.currency, "THB");
    assert_eq!(snapshot.shopify.minor, 400);
    assert_eq!(snapshot.shopify.currency, "AUD");
    let detached = snapshot.attributes();
    assert_eq!(detached.len(), 4);
    for attribute in detached {
        assert!(
            attrs
                .iter()
                .any(|actual| actual.key == attribute.key && actual.value == attribute.value)
        );
    }
    assert!(identity_matches(&row, &attrs));
    for key in MONEY_KEYS {
        let mut missing = attrs.clone();
        missing.retain(|attr| attr.key != key);
        assert!(parse_money_snapshots(&missing).is_err());
        let mut duplicate = attrs.clone();
        duplicate.push(attrs.iter().find(|attr| attr.key == key).unwrap().clone());
        assert!(parse_money_snapshots(&duplicate).is_err());
    }
    for (key, value) in [
        (PAYMENT_AMOUNT_KEY, "012000"),
        (PAYMENT_AMOUNT_KEY, "-1"),
        (PAYMENT_AMOUNT_KEY, "1.00"),
        (PAYMENT_AMOUNT_KEY, "9223372036854775808"),
        (PAYMENT_CURRENCY_KEY, "thb"),
        (SHOPIFY_CURRENCY_KEY, "ZZZ"),
    ] {
        let mut changed = attrs.clone();
        changed
            .iter_mut()
            .find(|attr| attr.key == key)
            .unwrap()
            .value = value.into();
        assert!(parse_money_snapshots(&changed).is_err());
    }
    let mut wrong_version = attrs;
    wrong_version
        .iter_mut()
        .find(|attr| attr.key == VERSION_KEY)
        .unwrap()
        .value = "2".into();
    assert!(parse_identity(&wrong_version).is_err());
}

#[test]
fn draft_identity_supports_payment_snapshot_before_shopify_snapshot_exists() {
    let mut row = row();
    row.correlation_version = INDEPENDENT_VERSION;
    row.payment_amount_minor = Some(12000);
    row.payment_currency = Some("THB".into());
    let attrs = identity_attributes(&row).unwrap();
    assert!(identity_matches(&row, &attrs));
    assert_eq!(parse_payment_money(&attrs).unwrap().unwrap().minor, 12000);
    assert!(parse_money_snapshots(&attrs).is_err());
    let mut legacy = row.clone();
    legacy.correlation_version = METADATA_VERSION;
    assert!(!identity_matches(&legacy, &attrs));
}

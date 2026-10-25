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

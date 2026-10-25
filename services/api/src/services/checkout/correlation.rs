use crate::{
    error::AppError,
    models::{checkout::Checkout, shopify::CustomAttribute},
};
use uuid::Uuid;

pub(crate) const LEGACY_VERSION: i16 = 1;
pub(crate) const METADATA_VERSION: i16 = 2;
pub(crate) const VERSION_KEY: &str = "checkout_correlation_version";
pub(crate) const CHECKOUT_KEY: &str = "checkout_id";
pub(crate) const CONVENTION_KEY: &str = "convention_id";
pub(crate) const SLUG_KEY: &str = "convention_slug";
pub(crate) const PROVIDER_KEY: &str = "payment_provider";
pub(crate) const STRIPE_SESSION_KEY: &str = "stripe_checkout_session_id";
pub(crate) const STRIPE_INTENT_KEY: &str = "stripe_payment_intent_id";
pub(crate) const STRIPE_CHARGE_KEY: &str = "stripe_charge_id";
pub(crate) const PAYPAL_ORDER_KEY: &str = "paypal_order_id";
pub(crate) const PAYPAL_CAPTURE_KEY: &str = "paypal_capture_id";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Identity {
    pub checkout_id: Uuid,
    pub convention_id: Uuid,
    pub convention_slug: String,
    pub provider: String,
}

impl Identity {
    pub(crate) fn from_checkout(checkout: &Checkout) -> Result<Self, AppError> {
        let identity = Self {
            checkout_id: checkout.id,
            convention_id: checkout.convention_id,
            convention_slug: checkout.convention_slug.clone().ok_or_else(invalid)?,
            provider: checkout.provider.clone(),
        };
        validate_identity(&identity)?;
        Ok(identity)
    }

    pub(crate) fn attributes(&self) -> Vec<CustomAttribute> {
        [
            (VERSION_KEY, METADATA_VERSION.to_string()),
            (CHECKOUT_KEY, self.checkout_id.to_string()),
            (CONVENTION_KEY, self.convention_id.to_string()),
            (SLUG_KEY, self.convention_slug.clone()),
            (PROVIDER_KEY, self.provider.clone()),
        ]
        .into_iter()
        .map(|(key, value)| CustomAttribute {
            key: key.into(),
            value,
        })
        .collect()
    }
}

pub(crate) fn identity_attributes(checkout: &Checkout) -> Result<Vec<CustomAttribute>, AppError> {
    Ok(Identity::from_checkout(checkout)?.attributes())
}

pub(crate) fn identity_matches(checkout: &Checkout, attributes: &[CustomAttribute]) -> bool {
    Identity::from_checkout(checkout).is_ok_and(|expected| {
        parse_identity(attributes).is_ok_and(|actual| actual.as_ref() == Some(&expected))
    })
}

pub(crate) fn unique_value<'a>(
    attributes: &'a [CustomAttribute],
    key: &str,
) -> Result<Option<&'a str>, AppError> {
    let mut matches = attributes.iter().filter(|attribute| attribute.key == key);
    let Some(attribute) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some()
        || attribute.value.trim().is_empty()
        || attribute.value.chars().any(char::is_control)
    {
        return Err(invalid());
    }
    Ok(Some(attribute.value.as_str()))
}

pub(crate) fn parse_identity(attributes: &[CustomAttribute]) -> Result<Option<Identity>, AppError> {
    let owned = [
        VERSION_KEY,
        CHECKOUT_KEY,
        CONVENTION_KEY,
        SLUG_KEY,
        PROVIDER_KEY,
        STRIPE_SESSION_KEY,
        STRIPE_INTENT_KEY,
        STRIPE_CHARGE_KEY,
        PAYPAL_ORDER_KEY,
        PAYPAL_CAPTURE_KEY,
    ];
    for key in owned {
        unique_value(attributes, key)?;
    }
    if !attributes
        .iter()
        .any(|attribute| attribute.key != SLUG_KEY && owned.contains(&attribute.key.as_str()))
    {
        return Ok(None);
    }
    if unique_value(attributes, VERSION_KEY)? != Some("2") {
        return Err(invalid());
    }
    let required = |key| unique_value(attributes, key)?.ok_or_else(invalid);
    let identity = Identity {
        checkout_id: canonical_uuid(required(CHECKOUT_KEY)?)?,
        convention_id: canonical_uuid(required(CONVENTION_KEY)?)?,
        convention_slug: required(SLUG_KEY)?.into(),
        provider: required(PROVIDER_KEY)?.into(),
    };
    validate_identity(&identity)?;
    let opposite: &[&str] = if identity.provider == "stripe" {
        &[PAYPAL_ORDER_KEY, PAYPAL_CAPTURE_KEY]
    } else {
        &[STRIPE_SESSION_KEY, STRIPE_INTENT_KEY, STRIPE_CHARGE_KEY]
    };
    for key in opposite {
        if unique_value(attributes, key)?.is_some() {
            return Err(invalid());
        }
    }
    for (key, kind) in [
        (STRIPE_SESSION_KEY, "stripe:session"),
        (STRIPE_INTENT_KEY, "stripe:intent"),
        (STRIPE_CHARGE_KEY, "stripe:charge"),
        (PAYPAL_ORDER_KEY, "paypal:order"),
        (PAYPAL_CAPTURE_KEY, "paypal:capture"),
    ] {
        if let Some(value) = unique_value(attributes, key)? {
            crate::utils::payment_reference::tag(kind, value).map_err(|_| invalid())?;
        }
    }
    Ok(Some(identity))
}

pub(crate) fn validate_legacy_identity(
    identity: &Identity,
    tags: &[String],
) -> Result<(), AppError> {
    for (prefix, expected) in [
        ("co:", identity.checkout_id),
        ("cv:", identity.convention_id),
    ] {
        let mut values = tags.iter().filter_map(|tag| tag.strip_prefix(prefix));
        if let Some(value) = values.next()
            && (values.next().is_some() || canonical_uuid(value)? != expected)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn validate_identity(identity: &Identity) -> Result<(), AppError> {
    crate::validations::conventions::slug(&identity.convention_slug).map_err(|_| invalid())?;
    if !matches!(identity.provider.as_str(), "stripe" | "paypal") {
        return Err(invalid());
    }
    Ok(())
}

fn canonical_uuid(value: &str) -> Result<Uuid, AppError> {
    let id = Uuid::parse_str(value).map_err(|_| invalid())?;
    if id.to_string() != value {
        return Err(invalid());
    }
    Ok(id)
}

fn invalid() -> AppError {
    crate::models::checkout_completion::evidence_conflict(
        "Shopify checkout metadata is missing or inconsistent",
    )
}

#[cfg(test)]
#[path = "../../tests/services/checkout/correlation.rs"]
mod tests;

use crate::{
    error::AppError,
    models::{CountryCode, shopify::ShopifyId},
    validations,
};
use axum::{
    body::Bytes,
    extract::{
        Path,
        rejection::{BytesRejection, PathRejection},
    },
};
use domain::dto::QuoteConventionRequest;

pub(crate) fn slug(path: Result<Path<String>, PathRejection>) -> Result<String, AppError> {
    let Path(slug) =
        path.map_err(|_| validations::conventions::invalid("Invalid convention slug"))?;
    validations::conventions::slug(&slug)?;
    Ok(slug)
}

pub(crate) fn country(value: &str) -> Option<String> {
    if value.len() != 2 || !value.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let code = value.to_ascii_uppercase();
    (!matches!(CountryCode::from(code.clone()), CountryCode::Other(_))).then_some(code)
}

pub(crate) fn quote(value: QuoteConventionRequest) -> Result<QuoteConventionRequest, AppError> {
    if value.items.len() > 250 {
        return Err(validations::conventions::invalid(
            "At most 250 cart lines are allowed",
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for item in &value.items {
        ShopifyId::new(&item.variant_id)
            .and_then(|id| id.number("ProductVariant"))
            .map_err(|_| {
                validations::conventions::invalid("Expected a positive Shopify ProductVariant GID")
            })?;
        if !seen.insert(&item.variant_id) {
            return Err(validations::conventions::invalid("Duplicate cart variant"));
        }
        if !(1..=999).contains(&item.quantity) {
            return Err(validations::conventions::invalid(
                "Quantity must be between 1 and 999",
            ));
        }
    }
    Ok(value)
}

pub(crate) fn no_body(value: Result<Bytes, BytesRejection>) -> Result<(), AppError> {
    if !value
        .map_err(|source| {
            validations::conventions::invalid("Unexpected request body").with_source(source)
        })?
        .is_empty()
    {
        return Err(validations::conventions::invalid("Unexpected request body"));
    }
    Ok(())
}

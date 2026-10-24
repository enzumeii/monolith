use super::conventions::invalid;
use crate::{
    error::AppError,
    models::{CountryCode, shopify::ShopifyId},
};
use axum::extract::{Path, rejection::PathRejection};
use domain::dto::QuoteConventionRequest;

pub(crate) fn slug(path: Result<Path<String>, PathRejection>) -> Result<String, AppError> {
    let Path(slug) = path.map_err(|_| invalid("Invalid convention slug"))?;
    super::conventions::slug(&slug)?;
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
        return Err(invalid("At most 250 cart lines are allowed"));
    }
    let mut seen = std::collections::HashSet::new();
    for item in &value.items {
        ShopifyId::new(&item.variant_id)
            .and_then(|id| id.number("ProductVariant"))
            .map_err(|_| invalid("Expected a positive Shopify ProductVariant GID"))?;
        if !seen.insert(&item.variant_id) {
            return Err(invalid("Duplicate cart variant"));
        }
        if !(1..=999).contains(&item.quantity) {
            return Err(invalid("Quantity must be between 1 and 999"));
        }
    }
    Ok(value)
}

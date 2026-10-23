use crate::{error::AppError, services::conventions::money};
use chrono::{DateTime, NaiveDate, Utc};
use domain::dto::{ConventionPaymentMethod, ConventionVariantDTO, ConventionVariantInput};
#[derive(Clone, Debug, sqlx::FromRow)]
pub(crate) struct Convention {
    pub id: uuid::Uuid,
    pub slug: String,
    pub name: String,
    pub logo_location: Option<String>,
    pub country_code: String,
    pub event_starts_on: NaiveDate,
    pub event_ends_on: NaiveDate,
    pub description_markdown: String,
    pub preorder_enabled: bool,
    pub currency_code: String,
    pub payment_methods: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Clone, Debug, sqlx::FromRow)]
pub(crate) struct ConventionVariant {
    pub product_title: Option<String>,
    pub variant_title: Option<String>,
    pub sku: Option<String>,
    pub product_image_url: Option<String>,
    pub variant_image_url: Option<String>,
    pub inventory_policy: Option<String>,
    pub id: uuid::Uuid,
    pub convention_id: uuid::Uuid,
    pub shopify_product_id: String,
    pub shopify_variant_id: String,
    pub final_price: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Clone, Debug)]
pub(crate) struct ConventionInput {
    pub slug: String,
    pub name: String,
    pub logo_location: Option<String>,
    pub country_code: String,
    pub event_starts_on: NaiveDate,
    pub event_ends_on: NaiveDate,
    pub description_markdown: String,
    pub currency_code: String,
    pub payment_methods: Vec<ConventionPaymentMethod>,
    pub preorder_enabled: bool,
}
impl Convention {
    pub(crate) fn methods(&self) -> Result<Vec<ConventionPaymentMethod>, AppError> {
        self.payment_methods
            .iter()
            .map(|s| {
                serde_json::from_value(serde_json::Value::String(s.clone()))
                    .map_err(|error| AppError::from(anyhow::Error::new(error)))
            })
            .collect()
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct VariantDisplay {
    pub inventory_policy: Option<domain::dto::CatalogueInventoryPolicy>,
    pub product_title: Option<String>,
    pub variant_title: Option<String>,
    pub sku: Option<String>,
    pub product_image: Option<domain::dto::CatalogueImage>,
    pub variant_image: Option<domain::dto::CatalogueImage>,
}
#[derive(Clone, Debug)]
pub(crate) struct SelectedVariant {
    pub input: ConventionVariantInput,
    pub display: VariantDisplay,
}
impl ConventionVariant {
    pub(crate) fn display(&self) -> Result<VariantDisplay, AppError> {
        let inventory_policy = self
            .inventory_policy
            .as_ref()
            .map(|value| {
                serde_json::from_value(serde_json::Value::String(value.clone()))
                    .map_err(|error| AppError::from(anyhow::Error::new(error)))
            })
            .transpose()?;
        Ok(VariantDisplay {
            inventory_policy,
            product_title: self.product_title.clone(),
            variant_title: self.variant_title.clone(),
            sku: self.sku.clone(),
            product_image: self
                .product_image_url
                .clone()
                .map(|url| domain::dto::CatalogueImage {
                    url,
                    alt_text: None,
                }),
            variant_image: self
                .variant_image_url
                .clone()
                .map(|url| domain::dto::CatalogueImage {
                    url,
                    alt_text: None,
                }),
        })
    }
    pub(crate) fn input(&self) -> ConventionVariantInput {
        ConventionVariantInput {
            shopify_product_id: self.shopify_product_id.clone(),
            shopify_variant_id: self.shopify_variant_id.clone(),
            final_price: self.final_price.clone(),
        }
    }
    pub(crate) fn into_dto(self, currency: &str) -> Result<ConventionVariantDTO, AppError> {
        let display = self.display()?;
        Ok(ConventionVariantDTO {
            inventory_policy: display.inventory_policy,
            product_title: display.product_title,
            variant_title: display.variant_title,
            sku: display.sku,
            product_image: display.product_image,
            variant_image: display.variant_image,
            id: self.id.to_string(),
            convention_id: self.convention_id.to_string(),
            shopify_product_id: self.shopify_product_id,
            shopify_variant_id: self.shopify_variant_id,
            final_price: money::canonical(&self.final_price, currency)
                .map_err(|error| AppError::from(anyhow::Error::new(error)))?,
            created_at: self.created_at.to_rfc3339(),
            updated_at: self.updated_at.to_rfc3339(),
        })
    }
}
pub(crate) fn method_values(methods: &[ConventionPaymentMethod]) -> Vec<String> {
    methods
        .iter()
        .map(|m| {
            match m {
                ConventionPaymentMethod::Stripe => "stripe",
                ConventionPaymentMethod::Paypal => "paypal",
                ConventionPaymentMethod::Promptpay => "promptpay",
            }
            .to_owned()
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/models/conventions.rs"]
mod tests;

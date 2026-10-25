use crate::error::AppError;
use chrono::{DateTime, NaiveDate, Utc};
use domain::dto::{ConventionPaymentMethod, ConventionVariantInput, CreateConvention, PatchField};
#[derive(Clone, Debug, sqlx::FromRow)]
pub(crate) struct Convention {
    pub id: uuid::Uuid,
    pub slug: String,
    pub name: String,
    pub logo_url: Option<String>,
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
    pub logo_url: Option<String>,
    pub country_code: String,
    pub event_starts_on: NaiveDate,
    pub event_ends_on: NaiveDate,
    pub description_markdown: String,
    pub currency_code: String,
    pub payment_methods: Vec<ConventionPaymentMethod>,
    pub preorder_enabled: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct CheckedConventionCreate {
    pub request: CreateConvention,
    pub event_starts_on: NaiveDate,
    pub event_ends_on: NaiveDate,
}
#[derive(Clone, Debug)]
pub(crate) struct CheckedConventionPatch {
    pub name: PatchField<String>,
    pub logo_url: PatchField<String>,
    pub country_code: PatchField<String>,
    pub event_starts_on: PatchField<NaiveDate>,
    pub event_ends_on: PatchField<NaiveDate>,
    pub description_markdown: PatchField<String>,
    pub payment_methods: PatchField<Vec<ConventionPaymentMethod>>,
    pub preorder_enabled: PatchField<bool>,
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
                .map(|url| domain::dto::CatalogueImage { url }),
            variant_image: self
                .variant_image_url
                .clone()
                .map(|url| domain::dto::CatalogueImage { url }),
        })
    }
    pub(crate) fn input(&self) -> ConventionVariantInput {
        ConventionVariantInput {
            shopify_product_id: self.shopify_product_id.clone(),
            shopify_variant_id: self.shopify_variant_id.clone(),
            final_price: self.final_price.clone(),
        }
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

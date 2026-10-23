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
impl ConventionVariant {
    pub(crate) fn input(&self) -> ConventionVariantInput {
        ConventionVariantInput {
            shopify_product_id: self.shopify_product_id.clone(),
            shopify_variant_id: self.shopify_variant_id.clone(),
            final_price: self.final_price.clone(),
        }
    }
    pub(crate) fn into_dto(self, currency: &str) -> Result<ConventionVariantDTO, AppError> {
        Ok(ConventionVariantDTO {
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

use super::PatchField;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ConventionPaymentMethod {
    Stripe,
    Paypal,
    Promptpay,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreateConvention {
    pub slug: String,
    pub name: String,
    pub logo_url: Option<String>,
    pub country_code: String,
    pub event_starts_on: String,
    pub event_ends_on: String,
    pub description_markdown: String,
    pub currency_code: String,
    pub payment_methods: Vec<ConventionPaymentMethod>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PatchConvention {
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub name: PatchField<String>,
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub logo_url: PatchField<String>,
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub country_code: PatchField<String>,
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub event_starts_on: PatchField<String>,
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub event_ends_on: PatchField<String>,
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub description_markdown: PatchField<String>,
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub payment_methods: PatchField<Vec<ConventionPaymentMethod>>,
    #[serde(skip_serializing_if = "PatchField::is_missing")]
    pub preorder_enabled: PatchField<bool>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ListConventionsQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub preorder_enabled: Option<bool>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ListConventionVariantsQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConventionVariantInput {
    pub shopify_product_id: String,
    pub shopify_variant_id: String,
    pub final_price: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReplaceConventionVariants {
    pub items: Vec<ConventionVariantInput>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ConventionDTO {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub logo_url: Option<String>,
    pub country_code: String,
    pub event_starts_on: String,
    pub event_ends_on: String,
    pub description_markdown: String,
    pub preorder_enabled: bool,
    pub currency_code: String,
    pub payment_methods: Vec<ConventionPaymentMethod>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ConventionVariantDTO {
    pub id: String,
    pub convention_id: String,
    pub shopify_product_id: String,
    pub shopify_variant_id: String,
    pub final_price: String,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConventionListResponse {
    pub items: Vec<ConventionDTO>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConventionVariantListResponse {
    pub items: Vec<ConventionVariantDTO>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConventionVariantsResponse {
    pub items: Vec<ConventionVariantDTO>,
}

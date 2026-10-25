use crate::{
    client::s3::PresignedDownload,
    models::conventions::{
        CheckedConventionCreate, Convention, ConventionInput, ConventionVariant, VariantDisplay,
    },
};
use domain::dto::{ConventionDTO, ConventionPaymentMethod, ConventionVariantDTO};

pub(crate) fn create(checked: CheckedConventionCreate) -> ConventionInput {
    let value = checked.request;
    ConventionInput {
        slug: value.slug,
        name: value.name,
        logo_url: value.logo_url,
        country_code: value.country_code,
        event_starts_on: checked.event_starts_on,
        event_ends_on: checked.event_ends_on,
        description_markdown: value.description_markdown,
        currency_code: value.currency_code,
        payment_methods: value.payment_methods,
        preorder_enabled: false,
    }
}

pub(crate) fn response(
    record: Convention,
    preview: Option<PresignedDownload>,
    payment_methods: Vec<ConventionPaymentMethod>,
) -> ConventionDTO {
    let (logo_preview_url, logo_preview_url_expires_at) = preview.map_or((None, None), |p| {
        (Some(p.url), Some(p.expires_at.to_rfc3339()))
    });
    ConventionDTO {
        id: record.id.to_string(),
        slug: record.slug,
        name: record.name,
        logo_preview_url,
        logo_preview_url_expires_at,
        country_code: record.country_code,
        event_starts_on: record.event_starts_on.to_string(),
        event_ends_on: record.event_ends_on.to_string(),
        description_markdown: record.description_markdown,
        preorder_enabled: record.preorder_enabled,
        currency_code: record.currency_code,
        payment_methods,
        created_at: record.created_at.to_rfc3339(),
        updated_at: record.updated_at.to_rfc3339(),
    }
}

pub(crate) fn variant(
    record: ConventionVariant,
    display: VariantDisplay,
    final_price: String,
) -> ConventionVariantDTO {
    ConventionVariantDTO {
        inventory_policy: display.inventory_policy,
        product_title: display.product_title,
        variant_title: display.variant_title,
        sku: display.sku,
        product_image: display.product_image,
        variant_image: display.variant_image,
        id: record.id.to_string(),
        convention_id: record.convention_id.to_string(),
        shopify_product_id: record.shopify_product_id,
        shopify_variant_id: record.shopify_variant_id,
        final_price,
        created_at: record.created_at.to_rfc3339(),
        updated_at: record.updated_at.to_rfc3339(),
    }
}

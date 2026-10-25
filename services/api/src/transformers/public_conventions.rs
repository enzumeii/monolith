use crate::models::shopify::variant_nodes::LiveVariant;
use domain::dto::{ConventionDTO, PublicConvention, PublicProduct, PublicVariant};

pub(crate) fn convention(dto: ConventionDTO) -> PublicConvention {
    PublicConvention {
        id: dto.id,
        slug: dto.slug,
        name: dto.name,
        logo_preview_url: dto.logo_preview_url,
        logo_preview_url_expires_at: dto.logo_preview_url_expires_at,
        country_code: dto.country_code,
        event_starts_on: dto.event_starts_on,
        event_ends_on: dto.event_ends_on,
        description_markdown: dto.description_markdown,
        currency_code: dto.currency_code,
    }
}

pub(crate) fn product(live: LiveVariant, variant: PublicVariant) -> PublicProduct {
    PublicProduct {
        id: live.product.id.as_ref().to_owned(),
        title: live.product.title,
        description: live.product.description,
        product_type: live.product.product_type,
        featured_image: live.product.featured_image,
        variants: vec![variant],
    }
}

pub(crate) fn variant(
    live: &LiveVariant,
    final_price: String,
    quantity: i64,
    max_quantity: u32,
) -> PublicVariant {
    PublicVariant {
        id: live.id.as_ref().to_owned(),
        title: live.title.clone(),
        sku: live.sku.clone(),
        image: live.image.clone(),
        selected_options: live.selected_options.clone(),
        final_price,
        inventory_policy: live.inventory_policy,
        available_quantity: quantity,
        max_quantity,
    }
}

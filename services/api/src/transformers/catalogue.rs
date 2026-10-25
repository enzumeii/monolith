use crate::models::shopify::catalogue::SelectionVariant;
use domain::dto::{CataloguePrice, CatalogueVariant};

pub(crate) fn variant(
    variant: SelectionVariant,
    quantity: Option<i64>,
    eligible: bool,
) -> CatalogueVariant {
    CatalogueVariant {
        id: variant.id.as_ref().to_owned(),
        inventory_item_id: variant.inventory_item.id.as_ref().to_owned(),
        title: variant.title,
        sku: variant.sku,
        image: variant.image,
        selected_options: variant.selected_options,
        price: CataloguePrice {
            amount: variant.contextual_pricing.price.amount,
            currency_code: variant.contextual_pricing.price.currency_code,
        },
        tracked: variant.inventory_item.tracked,
        inventory_policy: variant.inventory_policy,
        available_quantity: quantity,
        eligible,
    }
}

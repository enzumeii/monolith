use domain::dto::{
    ConventionQuote, PublicCatalogueResponse, PublicProduct, PublicVariant, QuoteConventionItem,
    QuoteConventionRequest,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cart {
    pub version: u8,
    pub convention_id: String,
    pub checkout_id: String,
    pub items: Vec<CartItem>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CartItem {
    pub variant_id: String,
    pub quantity: u32,
    pub accepted_unit_price: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReconciledLine {
    pub variant_id: String,
    pub title: String,
    pub variant_title: String,
    pub quantity: u32,
    pub max_quantity: u32,
    pub current_price: Option<String>,
    pub price_changed: bool,
    pub issue: Option<String>,
}

impl Cart {
    pub fn new(convention_id: String, checkout_id: String) -> Self {
        Self {
            version: 1,
            convention_id,
            checkout_id,
            items: Vec::new(),
        }
    }

    pub fn restore(raw: &str, convention_id: &str) -> Option<Self> {
        let cart: Self = serde_json::from_str(raw).ok()?;
        let mut ids = HashSet::new();
        (cart.version == 1
            && cart.convention_id == convention_id
            && valid_checkout_id(&cart.checkout_id)
            && cart.items.len() <= 250
            && cart.items.iter().all(|line| {
                line.variant_id.starts_with("gid://shopify/ProductVariant/")
                    && line
                        .variant_id
                        .rsplit('/')
                        .next()
                        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
                    && (1..=999).contains(&line.quantity)
                    && valid_price(&line.accepted_unit_price)
                    && ids.insert(line.variant_id.clone())
            }))
        .then_some(cart)
    }

    pub fn add(&mut self, variant: &PublicVariant, quantity: u32) -> Result<(), String> {
        let maximum = variant.max_quantity.min(999);
        if quantity == 0 {
            return Err("Choose a quantity of at least 1.".into());
        }
        if let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item.variant_id == variant.id)
        {
            let combined = item
                .quantity
                .checked_add(quantity)
                .filter(|&q| q <= maximum);
            item.quantity = combined
                .ok_or_else(|| format!("You can add at most {maximum} of this variant."))?;
        } else {
            if quantity == 0 || quantity > maximum || self.items.len() >= 250 {
                return Err(format!("Choose a quantity between 1 and {maximum}."));
            }
            self.items.push(CartItem {
                variant_id: variant.id.clone(),
                quantity,
                accepted_unit_price: variant.final_price.clone(),
            });
        }
        Ok(())
    }

    pub fn remove(&mut self, id: &str) {
        self.items.retain(|item| item.variant_id != id);
    }

    pub fn set_quantity(&mut self, id: &str, quantity: u32) {
        if let Some(item) = self.items.iter_mut().find(|item| item.variant_id == id) {
            item.quantity = quantity.clamp(1, 999);
        }
    }

    pub fn accept_prices(&mut self, products: &[PublicProduct]) {
        for item in &mut self.items {
            if let Some((_, variant)) = find_variant(products, &item.variant_id) {
                item.accepted_unit_price.clone_from(&variant.final_price);
            }
        }
    }

    pub fn quote_request(&self) -> QuoteConventionRequest {
        QuoteConventionRequest {
            items: self
                .items
                .iter()
                .map(|item| QuoteConventionItem {
                    variant_id: item.variant_id.clone(),
                    quantity: item.quantity,
                })
                .collect(),
        }
    }
}

pub fn reconciliation(cart: &Cart, products: &[PublicProduct]) -> Vec<ReconciledLine> {
    cart.items
        .iter()
        .map(|item| {
            let current = find_variant(products, &item.variant_id);
            let (title, variant_title, maximum, price, issue) = match current {
                Some((product, variant)) => {
                    let maximum = variant.max_quantity.min(999);
                    let issue = if maximum == 0 {
                        Some("Currently unavailable. Remove this item to update your total.".into())
                    } else if item.quantity > maximum {
                        Some(format!(
                            "Only {maximum} available. Reduce the quantity to update your total."
                        ))
                    } else {
                        None
                    };
                    (
                        product.title.clone(),
                        variant.title.clone(),
                        maximum,
                        Some(variant.final_price.clone()),
                        issue,
                    )
                }
                None => (
                    "Unavailable item".into(),
                    "Selected variant".into(),
                    0,
                    None,
                    Some(
                        "This selection is no longer available. Remove it to update your total."
                            .into(),
                    ),
                ),
            };
            ReconciledLine {
                variant_id: item.variant_id.clone(),
                title,
                variant_title,
                quantity: item.quantity,
                max_quantity: maximum,
                price_changed: price
                    .as_ref()
                    .is_some_and(|price| price != &item.accepted_unit_price),
                current_price: price,
                issue,
            }
        })
        .collect()
}

pub fn apply_quote_prices(
    cart: &Cart,
    expected_currency: &str,
    catalogue: &mut PublicCatalogueResponse,
    quote: &ConventionQuote,
) -> bool {
    if catalogue.convention.id != cart.convention_id
        || catalogue.convention.currency_code != expected_currency
        || quote.currency_code != expected_currency
        || quote.items.len() != cart.items.len()
    {
        return false;
    }
    let mut seen = HashSet::new();
    for line in &quote.items {
        if !seen.insert(&line.variant_id)
            || !cart
                .items
                .iter()
                .any(|item| item.variant_id == line.variant_id && item.quantity == line.quantity)
        {
            return false;
        }
        let Some(variant) = catalogue
            .products
            .iter_mut()
            .flat_map(|product| &mut product.variants)
            .find(|variant| variant.id == line.variant_id)
        else {
            return false;
        };
        variant.final_price.clone_from(&line.unit_price);
    }
    true
}

pub fn valid_checkout_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            14 => b == b'4',
            19 => matches!(b, b'8' | b'9' | b'a' | b'b'),
            _ => b.is_ascii_hexdigit() && !b.is_ascii_uppercase(),
        })
}

fn find_variant<'a>(
    products: &'a [PublicProduct],
    id: &str,
) -> Option<(&'a PublicProduct, &'a PublicVariant)> {
    products.iter().find_map(|product| {
        product
            .variants
            .iter()
            .find(|variant| variant.id == id)
            .map(|variant| (product, variant))
    })
}

fn valid_price(price: &str) -> bool {
    let mut dots = 0;
    !price.is_empty()
        && price.bytes().all(|b| {
            if b == b'.' {
                dots += 1;
                dots <= 1
            } else {
                b.is_ascii_digit()
            }
        })
        && !price.starts_with('.')
        && !price.ends_with('.')
}

#[cfg(test)]
#[path = "../tests/cart/model.rs"]
mod tests;

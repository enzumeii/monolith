use domain::dto::{
    ConventionDTO, ConventionPaymentMethod, ConventionVariantDTO, ConventionVariantInput,
    CreateConvention, PatchConvention, PatchField, ReplaceConventionVariants,
};
use std::collections::BTreeMap;

#[derive(Clone, Default, PartialEq)]
pub struct Metadata {
    pub slug: String,
    pub name: String,
    pub country: String,
    pub start: String,
    pub end: String,
    pub markdown: String,
    pub currency: String,
    pub methods: Vec<ConventionPaymentMethod>,
    pub enabled: bool,
}
impl Metadata {
    pub fn from_record(record: &ConventionDTO) -> Self {
        Self {
            slug: record.slug.clone(),
            name: record.name.clone(),
            country: record.country_code.clone(),
            start: record.event_starts_on.clone(),
            end: record.event_ends_on.clone(),
            markdown: record.description_markdown.clone(),
            currency: record.currency_code.clone(),
            methods: record.payment_methods.clone(),
            enabled: record.preorder_enabled,
        }
    }
    pub fn create(&self) -> Result<CreateConvention, String> {
        if !(1..=120).contains(&self.name.trim().chars().count()) || self.slug.is_empty() {
            return Err("Enter a name and slug".into());
        }
        if self.slug.len() > 63
            || !self
                .slug
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || self.slug.split('-').any(str::is_empty)
            || [
                "api",
                "v1",
                "healthz",
                "_cron",
                "backoffice",
                "user",
                "assets",
            ]
            .contains(&self.slug.as_str())
        {
            return Err(
                "Use a slug of at most 63 lowercase letters, numbers and internal hyphens".into(),
            );
        }
        if self.country.len() != 2 || !self.country.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err("Enter a two-letter uppercase country code".into());
        }
        let date = |value: &str| {
            chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()
                .filter(|_| value.len() == 10)
        };
        let Some(start) = date(&self.start) else {
            return Err("Enter an event start date".into());
        };
        let Some(end) = date(&self.end) else {
            return Err("Enter an event end date".into());
        };
        if start > end {
            return Err("Event end date must be on or after the start date".into());
        }
        if self.markdown.trim().is_empty()
            || self.markdown.len() > 20_000
            || self.markdown.contains('\0')
        {
            return Err(
                "Enter a nonblank Markdown description of at most 20000 UTF-8 bytes without NUL"
                    .into(),
            );
        }
        if self.currency.is_empty() || self.methods.is_empty() {
            return Err("Choose a supported currency and at least one payment method".into());
        }
        Ok(CreateConvention {
            slug: self.slug.clone(),
            name: self.name.trim().into(),
            logo_location: None,
            country_code: self.country.clone(),
            event_starts_on: self.start.clone(),
            event_ends_on: self.end.clone(),
            description_markdown: self.markdown.clone(),
            currency_code: self.currency.clone(),
            payment_methods: self.methods.clone(),
        })
    }
    pub fn patch(&self, saved: &Self) -> Result<PatchConvention, String> {
        if self == saved {
            return Err("No metadata changes to save".into());
        }
        let value = self.create()?;
        Ok(PatchConvention {
            name: if self.name == saved.name {
                PatchField::Missing
            } else {
                PatchField::Set(value.name)
            },
            country_code: if self.country == saved.country {
                PatchField::Missing
            } else {
                PatchField::Set(value.country_code)
            },
            event_starts_on: if self.start == saved.start {
                PatchField::Missing
            } else {
                PatchField::Set(value.event_starts_on)
            },
            event_ends_on: if self.end == saved.end {
                PatchField::Missing
            } else {
                PatchField::Set(value.event_ends_on)
            },
            description_markdown: if self.markdown == saved.markdown {
                PatchField::Missing
            } else {
                PatchField::Set(value.description_markdown)
            },
            payment_methods: if self.methods == saved.methods {
                PatchField::Missing
            } else {
                PatchField::Set(value.payment_methods)
            },
            preorder_enabled: if self.enabled == saved.enabled {
                PatchField::Missing
            } else {
                PatchField::Set(self.enabled)
            },
            ..Default::default()
        })
    }
    pub fn requires_capabilities(&self, saved: &Self) -> bool {
        self.methods != saved.methods || (self.enabled && !saved.enabled)
    }
}
#[derive(Clone, Default, PartialEq)]
pub struct Selection {
    pub items: BTreeMap<String, ConventionVariantInput>,
    pub loaded: bool,
}
impl Selection {
    pub fn from_saved<'a>(items: impl IntoIterator<Item = &'a ConventionVariantDTO>) -> Self {
        Self {
            loaded: true,
            items: items
                .into_iter()
                .map(|item| {
                    (
                        item.shopify_variant_id.clone(),
                        ConventionVariantInput {
                            shopify_product_id: item.shopify_product_id.clone(),
                            shopify_variant_id: item.shopify_variant_id.clone(),
                            final_price: item.final_price.clone(),
                        },
                    )
                })
                .collect(),
        }
    }
    pub fn insert(&mut self, product: String, variant: String) {
        self.items
            .entry(variant.clone())
            .or_insert(ConventionVariantInput {
                shopify_product_id: product,
                shopify_variant_id: variant,
                final_price: String::new(),
            });
    }
    pub fn request(&self) -> Result<ReplaceConventionVariants, String> {
        if !self.loaded {
            return Err("Load every saved membership page before saving".into());
        }
        if self.items.len() > 250 {
            return Err("Select no more than 250 variants".into());
        }
        if self
            .items
            .values()
            .any(|item| !positive_decimal(&item.final_price))
        {
            return Err("Enter a positive decimal price for every selected variant".into());
        }
        Ok(ReplaceConventionVariants {
            items: self.items.values().cloned().collect(),
        })
    }
}
#[derive(Clone, Default, PartialEq)]
pub struct VariantDetails {
    pub product_title: Option<String>,
    pub variant_title: Option<String>,
    pub sku: Option<String>,
    pub variant_image: Option<(String, String)>,
    pub available: Option<i64>,
    pub stock_location: Option<String>,
}
impl VariantDetails {
    pub fn apply_saved(&mut self, row: &crate::api::conventions::SelectedVariant) {
        apply_patch(&mut self.product_title, &row.product_title);
        apply_patch(&mut self.variant_title, &row.variant_title);
        apply_patch(&mut self.sku, &row.sku);
        match &row.variant_image {
            PatchField::Missing => {}
            PatchField::Clear => self.variant_image = None,
            PatchField::Set(image) => {
                self.variant_image = Some((image.url.clone(), String::new()));
            }
        }
        if let Some((_, label)) = &mut self.variant_image {
            self.variant_title
                .as_deref()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or("Variant image")
                .clone_into(label);
        }
        if !row.available_quantity.is_missing() {
            apply_patch(&mut self.available, &row.available_quantity);
            self.stock_location = None;
        }
    }
}
pub fn merge_saved_details(
    details: &mut BTreeMap<String, VariantDetails>,
    rows: &[crate::api::conventions::SelectedVariant],
) {
    details.retain(|id, _| {
        rows.iter()
            .any(|row| row.membership.shopify_variant_id == *id)
    });
    for row in rows {
        details
            .entry(row.membership.shopify_variant_id.clone())
            .or_default()
            .apply_saved(row);
    }
}
pub fn method_label(method: ConventionPaymentMethod) -> &'static str {
    match method {
        ConventionPaymentMethod::Stripe => "Stripe",
        ConventionPaymentMethod::Paypal => "PayPal",
        ConventionPaymentMethod::Promptpay => "PromptPay",
    }
}
pub fn positive_decimal(value: &str) -> bool {
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fractional = parts.next();
    !whole.is_empty()
        && whole.bytes().all(|b| b.is_ascii_digit())
        && fractional
            .is_none_or(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        && parts.next().is_none()
        && value.bytes().any(|b| matches!(b, b'1'..=b'9'))
}
fn apply_patch<T: Clone>(target: &mut Option<T>, value: &PatchField<T>) {
    match value {
        PatchField::Missing => {}
        PatchField::Clear => *target = None,
        PatchField::Set(value) => *target = Some(value.clone()),
    }
}
#[cfg(test)]
#[path = "../../tests/pages/conventions/state.rs"]
mod tests;

use domain::dto::PublicProduct;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub struct Filters {
    pub search: String,
    pub product_type: String,
    pub per_page: usize,
    pub page: usize,
}

impl Default for Filters {
    fn default() -> Self {
        Self {
            search: String::new(),
            product_type: String::new(),
            per_page: 24,
            page: 1,
        }
    }
}

impl Filters {
    pub fn matching<'a>(&self, products: &'a [PublicProduct]) -> Vec<&'a PublicProduct> {
        let query = self.search.trim().to_lowercase();
        products
            .iter()
            .filter(|product| {
                (self.product_type.is_empty() || self.product_type == product.product_type)
                    && (query.is_empty()
                        || [&product.title, &product.description, &product.product_type]
                            .into_iter()
                            .any(|text| text.to_lowercase().contains(&query))
                        || product.variants.iter().any(|variant| {
                            variant.title.to_lowercase().contains(&query)
                                || variant
                                    .sku
                                    .as_ref()
                                    .is_some_and(|sku| sku.to_lowercase().contains(&query))
                        }))
            })
            .collect()
    }

    pub fn page_count(&self, count: usize) -> usize {
        count.div_ceil(self.per_page).max(1)
    }

    pub fn visible_page(&self, count: usize) -> usize {
        self.page.clamp(1, self.page_count(count))
    }

    pub fn reset(&mut self) {
        self.page = 1;
    }
}

pub fn product_types(products: &[PublicProduct]) -> BTreeSet<String> {
    products
        .iter()
        .map(|product| product.product_type.clone())
        .filter(|kind| !kind.is_empty())
        .collect()
}

#[cfg(test)]
#[path = "../tests/filters.rs"]
mod tests;

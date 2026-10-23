use domain::dto::{CatalogueQuery, CatalogueResponse};
use reqwest::Method;

pub async fn products(query: CatalogueQuery) -> super::ApiResult<CatalogueResponse> {
    super::decode(super::request(Method::GET, "/admin/catalogue/products")?.query(&query)).await
}
#[cfg(test)]
#[path = "../tests/api/catalogue.rs"]
mod tests;

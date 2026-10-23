use super::{ApiResult, valid_uuid_id};
use domain::dto::{
    CatalogueImage, ConventionDTO, ConventionListResponse, ConventionOptionsResponse,
    ConventionVariantDTO, CreateConvention, ListConventionVariantsQuery, ListConventionsQuery,
    PatchConvention, PatchField, ReplaceConventionVariants,
};
use reqwest::Method;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct SelectedVariant {
    #[serde(flatten)]
    pub membership: ConventionVariantDTO,
    #[serde(default)]
    pub product_title: PatchField<String>,
    #[serde(default)]
    pub variant_title: PatchField<String>,
    #[serde(default)]
    pub sku: PatchField<String>,
    #[serde(default, rename = "product_image")]
    pub _product_image: PatchField<CatalogueImage>,
    #[serde(default)]
    pub variant_image: PatchField<CatalogueImage>,
    #[serde(default)]
    pub available_quantity: PatchField<i64>,
}
#[derive(Debug, Deserialize)]
pub struct SelectedVariantPage {
    pub items: Vec<SelectedVariant>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
}
#[derive(Debug, Deserialize)]
pub struct SelectedVariants {
    pub items: Vec<SelectedVariant>,
}

pub async fn list(query: ListConventionsQuery) -> ApiResult<ConventionListResponse> {
    super::decode(super::request(Method::GET, "/admin/conventions")?.query(&query)).await
}
pub async fn options() -> ApiResult<ConventionOptionsResponse> {
    super::decode(super::request(Method::GET, "/admin/conventions/options")?).await
}
pub async fn get(id: &str) -> ApiResult<ConventionDTO> {
    super::decode(super::uuid_request(Method::GET, "/admin/conventions", id)?).await
}
pub async fn create(input: CreateConvention) -> ApiResult<ConventionDTO> {
    super::decode(super::request(Method::POST, "/admin/conventions")?.json(&input)).await
}
pub async fn update(id: &str, input: PatchConvention) -> ApiResult<ConventionDTO> {
    super::decode(super::uuid_request(Method::PATCH, "/admin/conventions", id)?.json(&input)).await
}
pub async fn variants(id: &str, page: u32) -> ApiResult<SelectedVariantPage> {
    let request = child_request(Method::GET, id, "variants")?;
    super::decode(request.query(&ListConventionVariantsQuery {
        page: Some(page),
        per_page: Some(100),
    }))
    .await
}
pub async fn all_variants(id: &str) -> ApiResult<Vec<SelectedVariant>> {
    let mut items = Vec::new();
    for page in 1..=3 {
        let response = variants(id, page).await?;
        if response.page != page
            || response.per_page != 100
            || response.items.len() > 100
            || response
                .items
                .iter()
                .any(|item| item.membership.convention_id != id)
            || (response.has_next && response.items.is_empty())
        {
            return Err(super::ApiError::new(
                reqwest::StatusCode::BAD_GATEWAY,
                "Incomplete selected variants response; reload before saving",
            ));
        }
        items.extend(response.items);
        let unique: std::collections::HashSet<_> = items
            .iter()
            .map(|item| &item.membership.shopify_variant_id)
            .collect();
        if items.len() > 250 || unique.len() != items.len() {
            return Err(super::ApiError::new(
                reqwest::StatusCode::BAD_GATEWAY,
                "Invalid selected variants response; reload before saving",
            ));
        }
        if !response.has_next {
            return Ok(items);
        }
    }
    Err(super::ApiError::new(
        reqwest::StatusCode::BAD_GATEWAY,
        "Selected variants could not be loaded completely",
    ))
}
pub async fn replace_variants(
    id: &str,
    input: ReplaceConventionVariants,
) -> ApiResult<SelectedVariants> {
    super::decode(child_request(Method::PUT, id, "variants")?.json(&input)).await
}
pub fn logo_url(value: &str) -> Option<String> {
    let url = reqwest::Url::parse(value).ok()?;
    let authority = value
        .strip_prefix("http://")
        .and_then(|rest| rest.split(['/', '?', '#']).next());
    let local = authority.is_some_and(|authority| {
        ["localhost", "127.0.0.1", "[::1]"].iter().any(|host| {
            authority == *host
                || authority.strip_prefix(host).is_some_and(|suffix| {
                    suffix.starts_with(':') && suffix[1..].parse::<u16>().is_ok()
                })
        })
    });
    ((url.scheme() == "https" || (url.scheme() == "http" && local))
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none())
    .then(|| url.into())
}
fn child_request(method: Method, id: &str, child: &str) -> ApiResult<reqwest::RequestBuilder> {
    if !valid_uuid_id(id) {
        return Err(super::ApiError::new(
            reqwest::StatusCode::BAD_REQUEST,
            "Enter a valid record ID",
        ));
    }
    super::request(method, &format!("/admin/conventions/{id}/{child}"))
}
#[cfg(test)]
#[path = "../tests/api/conventions.rs"]
mod tests;

use super::{ApiResult, valid_uuid_id};
use domain::dto::{
    ConventionDTO, ConventionListResponse, ConventionOptionsResponse, ConventionVariantDTO,
    ConventionVariantListResponse, ConventionVariantsResponse, CreateConvention,
    ListConventionVariantsQuery, ListConventionsQuery, PatchConvention, ReplaceConventionVariants,
};
use reqwest::Method;

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
pub async fn variants(id: &str, page: u32) -> ApiResult<ConventionVariantListResponse> {
    let request = child_request(Method::GET, id, "variants")?;
    super::decode(request.query(&ListConventionVariantsQuery {
        page: Some(page),
        per_page: Some(100),
    }))
    .await
}
pub async fn all_variants(id: &str) -> ApiResult<Vec<ConventionVariantDTO>> {
    let mut items = Vec::new();
    for page in 1..=3 {
        let response = variants(id, page).await?;
        if response.page != page
            || response.per_page != 100
            || response.items.len() > 100
            || response.items.iter().any(|item| item.convention_id != id)
            || (response.has_next && response.items.is_empty())
        {
            return Err(super::ApiError::new(
                reqwest::StatusCode::BAD_GATEWAY,
                "Incomplete selected variants response; reload before saving",
            ));
        }
        items.extend(response.items);
        let unique: std::collections::HashSet<_> =
            items.iter().map(|item| &item.shopify_variant_id).collect();
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
) -> ApiResult<ConventionVariantsResponse> {
    super::decode(child_request(Method::PUT, id, "variants")?.json(&input)).await
}
pub async fn upload_logo(id: &str, content_type: &str, bytes: Vec<u8>) -> ApiResult<ConventionDTO> {
    super::decode(
        child_request(Method::PUT, id, "logo")?
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(bytes),
    )
    .await
}
pub fn logo_url(value: &str, api_base: &str) -> Option<String> {
    let parts: Vec<_> = value.split('/').collect();
    if parts.len() == 7
        && parts[..4] == ["", "api", "v1", "conventions"]
        && parts[5] == "logo"
        && valid_uuid_id(parts[4])
        && valid_uuid_id(parts[6])
    {
        return reqwest::Url::parse(api_base)
            .ok()?
            .join(value)
            .ok()
            .map(Into::into);
    }
    let url = reqwest::Url::parse(value).ok()?;
    (url.scheme() == "https"
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

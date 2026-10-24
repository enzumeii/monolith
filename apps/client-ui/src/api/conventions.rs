use super::{ApiError, ApiResult, decode, request};
use domain::dto::{
    ConventionQuote, ListPublicConventionsQuery, PublicCatalogueResponse, PublicConvention,
    PublicConventionListResponse, QuoteConventionRequest,
};
use reqwest::{Method, StatusCode};
use std::collections::HashSet;

pub async fn directory(
    today: String,
    mut locale_country: Option<String>,
) -> ApiResult<Vec<PublicConvention>> {
    let mut conventions = Vec::new();
    let mut seen = HashSet::new();
    let mut page = 1;
    loop {
        let query = ListPublicConventionsQuery {
            today: today.clone(),
            locale_country: locale_country.clone(),
            page: Some(page),
            per_page: Some(100),
        };
        let response: PublicConventionListResponse =
            match decode(request(Method::GET, "/conventions")?.query(&query)).await {
                Err(error) if error.status == 400 && locale_country.is_some() => {
                    locale_country = None;
                    conventions.clear();
                    seen.clear();
                    page = 1;
                    continue;
                }
                response => response?,
            };
        if response.page != page
            || response
                .items
                .iter()
                .any(|item| !seen.insert(item.id.clone()))
            || (response.has_next && response.items.is_empty())
        {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "The convention list changed while loading. Please retry.",
            ));
        }
        conventions.extend(response.items);
        if !response.has_next {
            return Ok(conventions);
        }
        page = page.checked_add(1).ok_or_else(|| {
            ApiError::new(StatusCode::BAD_GATEWAY, "Unable to load all conventions")
        })?;
    }
}

pub async fn catalogue(slug: &str) -> ApiResult<PublicCatalogueResponse> {
    validate_slug(slug)?;
    decode(request(
        Method::GET,
        &format!("/conventions/{slug}/products"),
    )?)
    .await
}

pub async fn quote(slug: &str, items: QuoteConventionRequest) -> ApiResult<ConventionQuote> {
    validate_slug(slug)?;
    decode(request(Method::POST, &format!("/conventions/{slug}/quote"))?.json(&items)).await
}

fn validate_slug(slug: &str) -> ApiResult<()> {
    if slug.is_empty()
        || slug.len() > 100
        || slug.starts_with('-')
        || slug.ends_with('-')
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "This convention is not available for preorders.",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/api/conventions.rs"]
mod tests;

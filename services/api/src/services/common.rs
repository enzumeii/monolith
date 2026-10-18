use crate::{
    client::{ClientError, shopify::ShopifyClient},
    error::AppError,
    state::AppState,
};

pub(super) fn shopify_client(state: &AppState) -> Result<&ShopifyClient, AppError> {
    state
        .shopify
        .as_ref()
        .map_err(|error| {
            AppError::ReportUnavailable(Some(error.clone())).at("api::client::shopify")
        })?
        .as_deref()
        .ok_or_else(|| AppError::ReportUnavailable(None).at(module_path!()))
}

pub(super) fn upstream(error: ClientError) -> AppError {
    let origin = match &error {
        ClientError::Origin { origin, .. } => *origin,
        _ => module_path!(),
    };
    AppError::ReportUpstream(error).at(origin)
}

#[cfg(test)]
#[path = "../tests/services/common.rs"]
pub(super) mod tests;

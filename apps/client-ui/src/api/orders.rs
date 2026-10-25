use super::{ApiError, ApiResult, decode_background, request};
use domain::dto::checkout::{CheckoutResponse, CreateCheckoutRequest};
use reqwest::{Method, StatusCode};

pub async fn create(body: &CreateCheckoutRequest, credential: &str) -> ApiResult<CheckoutResponse> {
    validate_identity(&body.operation_id, credential)?;
    decode_background(
        request(Method::POST, "/order/checkout")?
            .bearer_auth(credential)
            .json(body),
    )
    .await
}

pub async fn verify(id: &str, credential: &str) -> ApiResult<CheckoutResponse> {
    validate_identity(id, credential)?;
    decode_background(
        request(Method::POST, &format!("/order/{id}/verify"))?.bearer_auth(credential),
    )
    .await
}

fn validate_identity(id: &str, credential: &str) -> ApiResult<()> {
    if !crate::checkout::model::valid_id(id)
        || credential.len() != 64
        || !credential.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "This browser cannot recover this checkout. Keep your payment receipt and contact the shop.",
        ));
    }
    Ok(())
}

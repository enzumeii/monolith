use crate::error::AppError;
use axum::http::StatusCode;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

pub(crate) fn tag(kind: &str, id: &str) -> Result<String, AppError> {
    let tag = format!("payment:{kind}:{id}");
    if id.is_empty()
        || id
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, ':' | ','))
        || tag.chars().count() > 255
    {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "Invalid payment reference tag",
        ));
    }
    if tag.chars().count() > 40 {
        let digest = Sha256::digest(tag.as_bytes());
        return Ok(format!("payment:{}", URL_SAFE_NO_PAD.encode(&digest[..24])));
    }
    Ok(tag)
}

#[cfg(test)]
#[path = "../tests/utils/payment_reference.rs"]
mod tests;

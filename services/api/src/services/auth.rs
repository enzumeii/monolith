use crate::auth::Claims;
use crate::{
    error::AppError,
    repositories::{admin, auth},
    utils::password,
};
use chrono::{Duration, Utc};
use domain::dto::{AdminRole, LoginRequest, RefreshRequest, TokenResponse};
use jsonwebtoken::{EncodingKey, Header, encode};
use rand::RngExt;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

#[tracing::instrument(skip(pool, jwt_secret, req), fields(username = %req.username))]
pub async fn login(
    pool: &PgPool,
    jwt_secret: &[u8],
    req: LoginRequest,
) -> Result<TokenResponse, AppError> {
    let admin = admin::find_by_username(pool, &req.username)
        .await?
        .ok_or_else(|| {
            tracing::warn!("Authentication failed: admin not found");
            AppError::InvalidCredentials
        })?;

    if !password::verify(req.password, admin.password_hash).await? {
        tracing::warn!("Authentication failed: invalid password supplied");
        return Err(AppError::InvalidCredentials);
    }

    issue_token_pair(
        pool,
        jwt_secret,
        admin.id,
        admin.username,
        admin.role.into(),
    )
    .await
}

#[tracing::instrument(skip(pool, jwt_secret, req))]
pub async fn refresh(
    pool: &PgPool,
    jwt_secret: &[u8],
    req: RefreshRequest,
) -> Result<TokenResponse, AppError> {
    let incoming_hash = hash_token(&req.refresh_token);

    let token_record = auth::consume_refresh_token(pool, &incoming_hash)
        .await?
        .ok_or(AppError::InvalidToken)?;

    issue_token_pair(
        pool,
        jwt_secret,
        token_record.admin_id,
        token_record.username,
        token_record.role.into(),
    )
    .await
}

#[tracing::instrument(skip(pool))]
pub async fn logout(pool: &PgPool, admin_id: i64) -> Result<(), AppError> {
    auth::delete_all_refresh_tokens(pool, admin_id).await
}

#[tracing::instrument(skip(pool, jwt_secret))]
async fn issue_token_pair(
    pool: &PgPool,
    jwt_secret: &[u8],
    admin_id: i64,
    username: String,
    role: AdminRole,
) -> Result<TokenResponse, AppError> {
    let expiration: u64 = Utc::now()
        .checked_add_signed(Duration::minutes(10))
        .expect("valid timestamp")
        .timestamp()
        .try_into()
        .map_err(|_| anyhow::anyhow!("timestamp overflow"))?;
    let claims = Claims {
        admin_id,
        username,
        role,
        expiration,
    };
    let access_token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret),
    )
    .map_err(AppError::from)?;

    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    let refresh_token = hex::encode(bytes);
    let refresh_hash = hash_token(&refresh_token);
    let expires_at = Utc::now() + Duration::days(7);

    auth::create_refresh_token(pool, admin_id, &refresh_hash, expires_at).await?;

    Ok(TokenResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: 600, // 10 minutes in seconds
        id: admin_id,
    })
}

#[must_use]
fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn hash_token_is_deterministic_and_different_from_input() {
        let input = "test-token";
        let hash1 = hash_token(input);
        let hash2 = hash_token(input);
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, input);
    }
}

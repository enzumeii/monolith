use crate::{
    error::AppError,
    repositories::{admin, auth},
};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use chrono::{Duration, Utc};
use domain::{
    dto::{Claims, LoginRequest, RefreshRequest, TokenResponse},
    entities::AdminRole,
};
use jsonwebtoken::{EncodingKey, Header, encode};
use rand::RngExt;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

//////////////////////// TESTS ////////////////////////

//////////////////////// PURE CORE ////////////////////////

/// # Panics
///
/// Panics if the current timestamp cannot be represented (overflow), which is
/// unreachable in practice.
pub fn generate_jwt(
    jwt_secret: &[u8],
    admin_id: i64,
    username: String,
    role: AdminRole,
) -> Result<String, AppError> {
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

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret),
    )
    .map_err(AppError::from)
}

#[must_use]
pub fn generate_random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    hex::encode(bytes)
}

#[must_use]
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

//////////////////////// IMPERATIVE SHELL ////////////////////////

#[tracing::instrument(skip(pool, jwt_secret))]
async fn issue_token_pair(
    pool: &PgPool,
    jwt_secret: &[u8],
    admin_id: i64,
    username: String,
    role: AdminRole,
) -> Result<TokenResponse, AppError> {
    let access_token = generate_jwt(jwt_secret, admin_id, username, role)?;

    let rrefresh_token = generate_random_token();
    let refresh_hash = hash_token(&rrefresh_token);
    let expires_at = Utc::now() + Duration::days(7);

    auth::create_refresh_token(pool, admin_id, &refresh_hash, expires_at).await?;

    Ok(TokenResponse {
        access_token,
        refresh_token: rrefresh_token,
        token_type: "Bearer",
        expires_in: 600, // 10 minutes in seconds
        id: admin_id,
    })
}

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

    if !verify_password(req.password, admin.password_hash).await? {
        tracing::warn!("Authentication failed: invalid password supplied");
        return Err(AppError::InvalidCredentials);
    }

    issue_token_pair(pool, jwt_secret, admin.id, admin.username, admin.role).await
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
        token_record.role,
    )
    .await
}

#[tracing::instrument(skip(pool))]
pub async fn logout(pool: &PgPool, admin_id: i64) -> Result<(), AppError> {
    auth::delete_all_refresh_tokens(pool, admin_id).await
}

async fn verify_password(password: String, password_hash: String) -> Result<bool, AppError> {
    tokio::task::spawn_blocking(move || {
        let parsed_hash = PasswordHash::new(&password_hash)?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    })
    .await
    .map_err(|error| AppError::Internal(anyhow::Error::msg(error.to_string())))?
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use jsonwebtoken::{DecodingKey, Validation, decode};

    #[test]
    fn generate_jwt_creates_valid_token_with_claims() {
        let token = generate_jwt(b"test-secret", 1, "admin".to_string(), AdminRole::Admin).unwrap();
        let token_data = decode::<Claims>(
            &token,
            &DecodingKey::from_secret(b"test-secret"),
            &Validation::default(),
        )
        .unwrap();

        assert_eq!(token_data.claims.admin_id, 1);
        assert_eq!(token_data.claims.username, "admin");
        assert_eq!(token_data.claims.role, AdminRole::Admin);
    }

    #[test]
    fn generate_random_token_produces_hex_encoded_string() {
        let token = generate_random_token();
        assert_eq!(token.len(), 64);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_token_is_deterministic_and_different_from_input() {
        let input = "test-token";
        let hash1 = hash_token(input);
        let hash2 = hash_token(input);
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, input);
    }
}

use crate::{
    error::AppError,
    repositories::{admin::AdminRepository, auth::AuthRepository},
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

pub struct AuthService;

impl AuthService {
    #[tracing::instrument(skip(pool, jwt_secret, req), fields(username = %req.username))]
    pub async fn login(
        pool: &PgPool,
        jwt_secret: &[u8],
        req: LoginRequest,
    ) -> Result<TokenResponse, AppError> {
        let admin = AdminRepository::find_by_username(pool, &req.username)
            .await?
            .ok_or_else(|| {
                tracing::warn!("Authentication failed: admin not found");
                AppError::InvalidCredentials
            })?;

        let parsed_hash = PasswordHash::new(&admin.password_hash)?;
        if Argon2::default()
            .verify_password(req.password.as_bytes(), &parsed_hash)
            .is_err()
        {
            tracing::warn!("Authentication failed: invalid password supplied");
            return Err(AppError::InvalidCredentials);
        }

        Self::issue_token_pair(pool, jwt_secret, admin.id, admin.username, admin.role).await
    }

    #[tracing::instrument(skip(pool, jwt_secret, req))]
    pub async fn refresh(
        pool: &PgPool,
        jwt_secret: &[u8],
        req: RefreshRequest,
    ) -> Result<TokenResponse, AppError> {
        let incoming_hash = Self::hash_token(&req.refresh_token);

        let token_record = AuthRepository::find_refresh_token(pool, &incoming_hash)
            .await?
            .ok_or(AppError::InvalidToken)?;

        if token_record.revoked_at.is_some() || token_record.expires_at < Utc::now() {
            return Err(AppError::InvalidToken);
        }

        AuthRepository::revoke_refresh_token(pool, token_record.token_id).await?;

        Self::issue_token_pair(
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
        AuthRepository::delete_all_refresh_tokens(pool, admin_id).await
    }

    async fn issue_token_pair(
        pool: &PgPool,
        jwt_secret: &[u8],
        admin_id: i64,
        username: String,
        role: AdminRole,
    ) -> Result<TokenResponse, AppError> {
        let access_token = Self::generate_jwt(jwt_secret, admin_id, username, role)?;

        let raw_refresh_token = Self::generate_random_token();
        let refresh_hash = Self::hash_token(&raw_refresh_token);
        let expires_at = Utc::now() + Duration::days(7);

        AuthRepository::create_refresh_token(pool, admin_id, &refresh_hash, expires_at).await?;

        Ok(TokenResponse {
            access_token,
            refresh_token: raw_refresh_token,
            token_type: "Bearer",
            expires_in: 600, // 15 minutes in seconds
        })
    }

    fn generate_jwt(
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

    fn generate_random_token() -> String {
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        hex::encode(bytes)
    }

    fn hash_token(token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hex::encode(hasher.finalize())
    }
}

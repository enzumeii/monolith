use crate::{
    error::AppError,
    repositories::{admin::AdminRepositoryTrait, auth::AuthRepositoryTrait},
};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use domain::{
    dto::{Claims, LoginRequest, RefreshRequest, TokenResponse},
    entities::AdminRole,
};
use jsonwebtoken::{EncodingKey, Header, encode};
use rand::RngExt;
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait AuthServiceTrait: Send + Sync {
    async fn login(&self, jwt_secret: &[u8], req: LoginRequest) -> Result<TokenResponse, AppError>;

    async fn refresh(
        &self,
        jwt_secret: &[u8],
        req: RefreshRequest,
    ) -> Result<TokenResponse, AppError>;

    async fn logout(&self, admin_id: i64) -> Result<(), AppError>;
}

#[derive(Debug, Clone)]
pub struct AuthService {
    auth_repo: Arc<dyn AuthRepositoryTrait>,
    admin_repo: Arc<dyn AdminRepositoryTrait>,
}

impl AuthService {
    pub fn new(
        auth_repo: Arc<dyn AuthRepositoryTrait>,
        admin_repo: Arc<dyn AdminRepositoryTrait>,
    ) -> Self {
        Self {
            auth_repo,
            admin_repo,
        }
    }

    #[tracing::instrument(skip(self, jwt_secret))]
    async fn issue_token_pair(
        &self,
        jwt_secret: &[u8],
        admin_id: i64,
        username: String,
        role: AdminRole,
    ) -> Result<TokenResponse, AppError> {
        let access_token = generate_jwt(jwt_secret, admin_id, username, role)?;

        let rrefresh_token = generate_random_token();
        let refresh_hash = hash_token(&rrefresh_token);
        let expires_at = Utc::now() + Duration::days(7);

        self.auth_repo
            .create_refresh_token(admin_id, &refresh_hash, expires_at)
            .await?;

        Ok(TokenResponse {
            access_token,
            refresh_token: rrefresh_token,
            token_type: "Bearer",
            expires_in: 600, // 15 minutes in seconds
            id: admin_id,
        })
    }
}

#[async_trait]
impl AuthServiceTrait for AuthService {
    #[tracing::instrument(skip(self, jwt_secret, req), fields(username = %req.username))]
    async fn login(&self, jwt_secret: &[u8], req: LoginRequest) -> Result<TokenResponse, AppError> {
        let admin = self
            .admin_repo
            .find_by_username(&req.username)
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

        self.issue_token_pair(jwt_secret, admin.id, admin.username, admin.role)
            .await
    }

    #[tracing::instrument(skip(self, jwt_secret))]
    async fn refresh(
        &self,
        jwt_secret: &[u8],
        req: RefreshRequest,
    ) -> Result<TokenResponse, AppError> {
        let incoming_hash = hash_token(&req.refresh_token);

        let token_record = self
            .auth_repo
            .find_refresh_token(&incoming_hash)
            .await?
            .ok_or(AppError::InvalidToken)?;

        if token_record.revoked_at.is_some() || token_record.expires_at < Utc::now() {
            return Err(AppError::InvalidToken);
        }

        self.auth_repo
            .revoke_refresh_token(token_record.token_id)
            .await?;

        self.issue_token_pair(
            jwt_secret,
            token_record.admin_id,
            token_record.username,
            token_record.role,
        )
        .await
    }

    #[tracing::instrument(skip(self))]
    async fn logout(&self, admin_id: i64) -> Result<(), AppError> {
        self.auth_repo.delete_all_refresh_tokens(admin_id).await
    }
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

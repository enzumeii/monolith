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

        if !verify_password(req.password, admin.password_hash).await? {
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
            .consume_refresh_token(&incoming_hash)
            .await?
            .ok_or(AppError::InvalidToken)?;

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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use argon2::{
        PasswordHasher,
        password_hash::{SaltString, rand_core::OsRng},
    };
    use chrono::Utc;
    use domain::entities::{Admin, TokenRefreshRecord};

    use super::*;
    use crate::repositories::{admin::MockAdminRepositoryTrait, auth::MockAuthRepositoryTrait};

    fn admin_repo_with_password(raw_password: &str) -> MockAdminRepositoryTrait {
        let password_hash = Argon2::default()
            .hash_password(raw_password.as_bytes(), &SaltString::generate(&mut OsRng))
            .unwrap()
            .to_string();
        let mut admins = MockAdminRepositoryTrait::new();
        admins.expect_find_by_username().returning(move |_| {
            Ok(Some(Admin {
                id: 1,
                username: "admin".into(),
                password_hash: password_hash.clone(),
                role: AdminRole::Admin,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }))
        });
        admins
    }

    #[tokio::test]
    async fn login_rejects_invalid_password_without_issuing_a_token() {
        let admins = admin_repo_with_password("correct-password");
        let service = AuthService::new(Arc::new(MockAuthRepositoryTrait::new()), Arc::new(admins));

        let result = service
            .login(
                b"secret",
                LoginRequest {
                    username: "admin".into(),
                    password: "wrong-password".into(),
                },
            )
            .await;

        assert!(matches!(result, Err(AppError::InvalidCredentials)));
    }

    #[tokio::test]
    async fn login_issues_a_token_pair_and_persists_the_refresh_token_hash() {
        let admins = admin_repo_with_password("correct-password");
        let mut auth_repo = MockAuthRepositoryTrait::new();
        auth_repo
            .expect_create_refresh_token()
            .withf(|admin_id, _hash, _expires_at| *admin_id == 1)
            .returning(|_, _, _| Ok(()));
        let service = AuthService::new(Arc::new(auth_repo), Arc::new(admins));

        let response = service
            .login(
                b"secret",
                LoginRequest {
                    username: "admin".into(),
                    password: "correct-password".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(response.id, 1);
        assert_eq!(response.token_type, "Bearer");
        assert!(!response.refresh_token.is_empty());
        assert!(!response.access_token.is_empty());
    }

    #[tokio::test]
    async fn login_rejects_unknown_username_without_issuing_a_token() {
        let mut admins = MockAdminRepositoryTrait::new();
        admins.expect_find_by_username().returning(|_| Ok(None));
        let service = AuthService::new(Arc::new(MockAuthRepositoryTrait::new()), Arc::new(admins));

        let result = service
            .login(
                b"secret",
                LoginRequest {
                    username: "missing".into(),
                    password: "whatever".into(),
                },
            )
            .await;

        assert!(matches!(result, Err(AppError::InvalidCredentials)));
    }

    #[tokio::test]
    async fn refresh_returns_invalid_token_when_repository_finds_no_record() {
        let mut auth_repo = MockAuthRepositoryTrait::new();
        auth_repo
            .expect_consume_refresh_token()
            .returning(|_| Ok(None));
        let admins = MockAdminRepositoryTrait::new();
        let service = AuthService::new(Arc::new(auth_repo), Arc::new(admins));

        let result = service
            .refresh(
                b"secret",
                RefreshRequest {
                    refresh_token: "stale-token".into(),
                },
            )
            .await;

        assert!(matches!(result, Err(AppError::InvalidToken)));
    }

    #[tokio::test]
    async fn refresh_issues_a_replacement_pair_when_consumption_succeeds() {
        let mut auth_repo = MockAuthRepositoryTrait::new();
        auth_repo.expect_consume_refresh_token().returning(|_| {
            Ok(Some(TokenRefreshRecord {
                token_id: uuid::Uuid::new_v4(),
                admin_id: 3,
                username: "admin".into(),
                role: AdminRole::Admin,
                expires_at: Utc::now(),
                revoked_at: Some(Utc::now()),
            }))
        });
        auth_repo
            .expect_create_refresh_token()
            .withf(|admin_id, _hash, _expires_at| *admin_id == 3)
            .returning(|_, _, _| Ok(()));
        let admins = MockAdminRepositoryTrait::new();
        let service = AuthService::new(Arc::new(auth_repo), Arc::new(admins));

        let response = service
            .refresh(
                b"secret",
                RefreshRequest {
                    refresh_token: "valid-token".into(),
                },
            )
            .await
            .unwrap();

        assert_eq!(response.id, 3);
    }

    #[tokio::test]
    async fn logout_deletes_all_refresh_tokens_for_the_admin() {
        let mut auth_repo = MockAuthRepositoryTrait::new();
        auth_repo
            .expect_delete_all_refresh_tokens()
            .withf(|admin_id| *admin_id == 11)
            .returning(|_| Ok(()));
        let admins = MockAdminRepositoryTrait::new();
        let service = AuthService::new(Arc::new(auth_repo), Arc::new(admins));

        service.logout(11).await.unwrap();
    }
}

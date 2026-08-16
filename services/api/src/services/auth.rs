use crate::{error::AppError, repositories::admin::AdminRepository};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use chrono::{Duration, Utc};
use domain::{
    dto::{AdminLoginRequest, AdminLoginResponse, Claims},
    entities::Admin,
};
use jsonwebtoken::{EncodingKey, Header, encode};
use sqlx::PgPool;

pub struct AuthService;

impl AuthService {
    #[tracing::instrument(skip(pool, jwt_secret, req), fields(username = %req.username))]
    pub async fn login(
        pool: &PgPool,
        jwt_secret: &[u8],
        req: AdminLoginRequest,
    ) -> Result<AdminLoginResponse, AppError> {
        // 1. Fetch user via Repository
        let admin = AdminRepository::find_by_username(pool, &req.username).await?;

        // 2. Verify Password Hash
        let parsed_hash = PasswordHash::new(&admin.password_hash)?;
        if Argon2::default()
            .verify_password(req.password.as_bytes(), &parsed_hash)
            .is_err()
        {
            tracing::warn!("Authentication failed: invalid password supplied");
            return Err(AppError::InvalidCredentials);
        }

        // 3. Issue JWT Token
        let token = Self::generate_jwt(&admin, jwt_secret)?;

        Ok(AdminLoginResponse {
            access_token: token,
            token_type: "Bearer",
        })
    }

    fn generate_jwt(admin: &Admin, jwt_secret: &[u8]) -> Result<String, AppError> {
        let expiration: usize = Utc::now()
            .checked_add_signed(Duration::hours(24))
            .expect("valid timestamp")
            .timestamp()
            .try_into()
            .map_err(|_| anyhow::anyhow!("timestamp overflow"))?;

        let claims = Claims {
            sub: admin.id.to_string(),
            username: admin.username.clone(),
            exp: expiration,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(jwt_secret),
        )
        .map_err(AppError::from)
    }
}

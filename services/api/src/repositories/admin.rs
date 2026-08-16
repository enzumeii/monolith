use crate::error::AppError;
use domain::entities::Admin;
use sqlx::PgPool;

pub struct AdminRepository;

impl AdminRepository {
    #[tracing::instrument(skip(pool))]
    pub async fn find_by_username(pool: &PgPool, username: &str) -> Result<Admin, AppError> {
        let admin = sqlx::query_as!(
            Admin,
            r#"
                SELECT id, username, password_hash, created_at, updated_at
                FROM admin
                WHERE username = $1
            "#,
            username
        )
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| {
            tracing::warn!("Authentication failed: admin not found");
            AppError::InvalidCredentials
        })?;

        Ok(admin)
    }
}

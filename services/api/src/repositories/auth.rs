use crate::error::AppError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::{AdminRole, TokenRefreshRecord};
use sqlx::{PgPool, types::Uuid};
use std::fmt::Debug;

#[derive(Debug, Clone)]
pub struct PgAuthRepository {
    pool: PgPool,
}

impl PgAuthRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait AuthRepositoryTrait: Send + Sync + Debug {
    async fn create_refresh_token(
        &self,
        admin_id: i64,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), AppError>;

    async fn find_refresh_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<TokenRefreshRecord>, AppError>;

    async fn revoke_refresh_token(&self, token_id: Uuid) -> Result<(), AppError>;

    async fn delete_all_refresh_tokens(&self, admin_id: i64) -> Result<(), AppError>;
}

#[async_trait]
impl AuthRepositoryTrait for PgAuthRepository {
    #[tracing::instrument(skip(self))]
    async fn create_refresh_token(
        &self,
        admin_id: i64,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<(), AppError> {
        sqlx::query!(
            r#"
                INSERT INTO refresh_tokens (admin_id,token_hash,expires_at)
                VALUES ($1, $2, $3)
            "#,
            admin_id,
            token_hash,
            expires_at
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn find_refresh_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<TokenRefreshRecord>, AppError> {
        let record = sqlx::query_as!(
            TokenRefreshRecord,
            r#"
            SELECT 
                rt.id AS token_id,
                rt.admin_id,
                a.username,
                a.role AS "role: AdminRole",
                rt.expires_at,
                rt.revoked_at
            FROM refresh_tokens rt
            INNER JOIN admin a ON a.id = rt.admin_id
            WHERE rt.token_hash = $1
            "#,
            token_hash
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(record)
    }

    #[tracing::instrument(skip(self))]
    async fn revoke_refresh_token(&self, token_id: Uuid) -> Result<(), AppError> {
        sqlx::query!(
            r#"
            UPDATE refresh_tokens
            SET revoked_at = NOW()
            WHERE id = $1
            "#,
            token_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    async fn delete_all_refresh_tokens(&self, admin_id: i64) -> Result<(), AppError> {
        sqlx::query!(
            r#"
            DELETE FROM refresh_tokens
            WHERE admin_id = $1
            "#,
            admin_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

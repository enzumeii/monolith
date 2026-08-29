use chrono::{DateTime, Utc};
use domain::entities::TokenRefreshRecord;
use sqlx::{PgPool, types::Uuid};

use crate::error::AppError;

pub struct AuthRepository;

impl AuthRepository {
    pub async fn create_refresh_token(
        pool: &PgPool,
        admin_id: Uuid,
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
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn find_refresh_token(
        pool: &PgPool,
        token_hash: &str,
    ) -> Result<Option<TokenRefreshRecord>, AppError> {
        let record = sqlx::query_as!(
            TokenRefreshRecord,
            r#"
            SELECT 
                rt.id AS token_id,
                rt.admin_id,
                a.username,
                rt.expires_at,
                rt.revoked_at
            FROM refresh_tokens rt
            INNER JOIN admin a ON a.id = rt.admin_id
            WHERE rt.token_hash = $1
            "#,
            token_hash
        )
        .fetch_optional(pool)
        .await?;

        Ok(record)
    }

    pub async fn revoke_refresh_token(pool: &PgPool, token_id: Uuid) -> Result<(), AppError> {
        sqlx::query!(
            r#"
            UPDATE refresh_tokens
            SET revoked_at = NOW()
            WHERE id = $1
            "#,
            token_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn delete_all_refresh_tokens(pool: &PgPool, admin_id: Uuid) -> Result<(), AppError> {
        sqlx::query!(
            r#"
            DELETE FROM refresh_tokens
            WHERE admin_id = $1
            "#,
            admin_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }
}

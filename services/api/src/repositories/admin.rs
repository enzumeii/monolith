use crate::error::AppError;
use domain::entities::{Admin, AdminRole};
use sqlx::PgPool;

pub struct AdminRepository;

impl AdminRepository {
    #[tracing::instrument(skip(pool))]
    pub async fn list(pool: &PgPool) -> Result<Vec<Admin>, AppError> {
        let admins = sqlx::query_as!(
            Admin,
            r#"
                SELECT id, username, password_hash, role AS "role: AdminRole", created_at, updated_at
                FROM admin
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(admins)
    }

    #[tracing::instrument(skip(pool, password_hash))]
    pub async fn add(
        pool: &PgPool,
        username: String,
        password_hash: String,
        role: AdminRole,
    ) -> Result<Admin, AppError> {
        let admin = sqlx::query_as!(
            Admin,
            r#"
                INSERT INTO admin (username, password_hash, role)
                VALUES ($1, $2, $3::ADMIN_ROLE)
                RETURNING id, username, password_hash, role AS "role: AdminRole", created_at, updated_at
            "#,
            username,
            password_hash,
            role as AdminRole
        )
        .fetch_one(pool)
        .await?;

        Ok(admin)
    }

    #[tracing::instrument(skip(pool, password_hash))]
    pub async fn update(
        pool: &PgPool,
        id: i64,
        username: Option<String>,
        password_hash: Option<String>,
    ) -> Result<Option<Admin>, AppError> {
        let admin = sqlx::query_as!(
            Admin,
            r#"
                UPDATE admin
                SET 
                    username = COALESCE($2, username),
                    password_hash = COALESCE($3, password_hash),
                    updated_at = NOW()
                WHERE id = $1
                RETURNING id, username, password_hash, role AS "role: AdminRole", created_at, updated_at
            "#,
            id,
            username,
            password_hash
        )
        .fetch_optional(pool)
        .await?;

        Ok(admin)
    }

    #[tracing::instrument(skip(pool))]
    pub async fn find_by_username(
        pool: &PgPool,
        username: &str,
    ) -> Result<Option<Admin>, AppError> {
        let admin = sqlx::query_as!(
            Admin,
            r#"
                SELECT id, username, password_hash, role AS "role: AdminRole", created_at, updated_at
                FROM admin
                WHERE username = $1
            "#,
            username
        )
        .fetch_optional(pool)
        .await?;

        Ok(admin)
    }

    #[tracing::instrument(skip(pool))]
    pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<Admin>, AppError> {
        let admin = sqlx::query_as!(
            Admin,
            r#"
                SELECT id, username, password_hash, role AS "role: AdminRole", created_at, updated_at
                FROM admin
                WHERE id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(admin)
    }

    #[tracing::instrument(skip(pool))]
    pub async fn delete(pool: &PgPool, id: i64) -> Result<Option<Admin>, AppError> {
        let admin = sqlx::query_as!(
            Admin,
            r#"
                DELETE FROM admin 
                WHERE id = $1 AND role != 'SUPER_ADMIN'::ADMIN_ROLE
                RETURNING id, username, password_hash, role AS "role: AdminRole", created_at, updated_at
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(admin)
    }
}

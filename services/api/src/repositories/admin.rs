use crate::error::AppError;
use async_trait::async_trait;
use domain::{
    dto::AdminPageRequest,
    entities::{Admin, AdminRole},
};
use sqlx::PgPool;
use std::fmt::Debug;

#[derive(Debug, Clone)]
pub struct PgAdminRepository {
    pool: PgPool,
}

impl From<PgPool> for PgAdminRepository {
    fn from(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait AdminRepositoryTrait: Send + Sync + Debug {
    async fn list_page(&self, page: AdminPageRequest) -> Result<Vec<Admin>, AppError>;

    async fn add(
        &self,
        username: String,
        password_hash: String,
        role: AdminRole,
    ) -> Result<Admin, AppError>;

    async fn update(
        &self,
        id: i64,
        username: Option<String>,
        password_hash: Option<String>,
    ) -> Result<Option<Admin>, AppError>;

    async fn find_by_username<'a>(&self, username: &'a str) -> Result<Option<Admin>, AppError>;

    async fn find_by_id(&self, id: i64) -> Result<Option<Admin>, AppError>;

    async fn delete(&self, id: i64) -> Result<Option<Admin>, AppError>;
}

#[async_trait]
impl AdminRepositoryTrait for PgAdminRepository {
    #[tracing::instrument(skip(self))]
    async fn list_page(&self, page: AdminPageRequest) -> Result<Vec<Admin>, AppError> {
        let limit = i64::from(page.per_page) + 1;
        let offset = i64::from(page.page - 1) * i64::from(page.per_page);
        let admins = sqlx::query_as!(
            Admin,
            r#"
                SELECT id, username, password_hash, role AS "role: AdminRole", created_at, updated_at
                FROM admin
                ORDER BY id ASC
                LIMIT $1 OFFSET $2
            "#,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(admins)
    }

    #[tracing::instrument(skip(self, password_hash))]
    async fn add(
        &self,
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
        .fetch_one(&self.pool)
        .await?;

        Ok(admin)
    }

    #[tracing::instrument(skip(self, password_hash))]
    async fn update(
        &self,
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
        .fetch_optional(&self.pool)
        .await?;

        Ok(admin)
    }

    #[tracing::instrument(skip(self))]
    async fn find_by_username<'a>(&self, username: &'a str) -> Result<Option<Admin>, AppError> {
        let admin = sqlx::query_as!(
            Admin,
            r#"
                SELECT id, username, password_hash, role AS "role: AdminRole", created_at, updated_at
                FROM admin
                WHERE username = $1
            "#,
            username
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(admin)
    }

    #[tracing::instrument(skip(self))]
    async fn find_by_id(&self, id: i64) -> Result<Option<Admin>, AppError> {
        let admin = sqlx::query_as!(
            Admin,
            r#"
                SELECT id, username, password_hash, role AS "role: AdminRole", created_at, updated_at
                FROM admin
                WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(admin)
    }

    #[tracing::instrument(skip(self))]
    async fn delete(&self, id: i64) -> Result<Option<Admin>, AppError> {
        let admin = sqlx::query_as!(
            Admin,
            r#"
                DELETE FROM admin 
                WHERE id = $1 AND role != 'SUPER_ADMIN'::ADMIN_ROLE
                RETURNING id, username, password_hash, role AS "role: AdminRole", created_at, updated_at
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(admin)
    }
}

use crate::error::AppError;
use crate::{
    models::{Admin, DbAdminRole},
    validations::admin::AdminPageRequest,
};
use domain::dto::AdminRole;
use sqlx::PgPool;

#[tracing::instrument(skip(pool))]
pub async fn list_page(pool: &PgPool, page: AdminPageRequest) -> Result<Vec<Admin>, AppError> {
    let limit = i64::from(page.per_page()) + 1;
    let offset = i64::from(page.page() - 1) * i64::from(page.per_page());
    let admins = sqlx::query_as!(
        Admin,
        r#"
                SELECT id, username, password_hash, role AS "role: DbAdminRole", created_at, updated_at
                FROM admin
                ORDER BY id ASC
                LIMIT $1 OFFSET $2
            "#,
        limit,
        offset
    )
    .fetch_all(pool)
    .await.map_err(|error| AppError::from(error).at(concat!(module_path!(), "::list_page")))?;

    Ok(admins)
}

#[tracing::instrument(skip(connection, password_hash))]
pub(crate) async fn add(
    connection: &mut sqlx::PgConnection,
    username: String,
    password_hash: String,
    role: AdminRole,
) -> Result<Admin, AppError> {
    let admin = sqlx::query_as!(
        Admin,
        r#"
                INSERT INTO admin (username, password_hash, role)
                VALUES ($1, $2, $3::ADMIN_ROLE)
                RETURNING id, username, password_hash, role AS "role: DbAdminRole", created_at, updated_at
            "#,
        username,
        password_hash,
        DbAdminRole::from(role) as DbAdminRole
    )
    .fetch_one(connection)
    .await.map_err(|error| AppError::from(error).at(concat!(module_path!(), "::add")))?;

    Ok(admin)
}

#[tracing::instrument(skip(connection, password_hash))]
pub(crate) async fn update(
    connection: &mut sqlx::PgConnection,
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
                RETURNING id, username, password_hash, role AS "role: DbAdminRole", created_at, updated_at
            "#,
        id,
        username,
        password_hash
    )
    .fetch_optional(connection)
    .await.map_err(|error| AppError::from(error).at(concat!(module_path!(), "::update")))?;

    Ok(admin)
}

#[tracing::instrument(skip(pool))]
pub async fn find_by_username(pool: &PgPool, username: &str) -> Result<Option<Admin>, AppError> {
    let admin = sqlx::query_as!(
        Admin,
        r#"
                SELECT id, username, password_hash, role AS "role: DbAdminRole", created_at, updated_at
                FROM admin
                WHERE username = $1
            "#,
        username
    )
    .fetch_optional(pool)
    .await.map_err(|error| AppError::from(error).at(concat!(module_path!(), "::find_by_username")))?;

    Ok(admin)
}

#[tracing::instrument(skip(pool))]
pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<Admin>, AppError> {
    let admin = sqlx::query_as!(
        Admin,
        r#"
                SELECT id, username, password_hash, role AS "role: DbAdminRole", created_at, updated_at
                FROM admin
                WHERE id = $1
            "#,
        id
    )
    .fetch_optional(pool)
    .await.map_err(|error| AppError::from(error).at(concat!(module_path!(), "::find_by_id")))?;

    Ok(admin)
}

#[tracing::instrument(skip(connection))]
pub(crate) async fn delete(
    connection: &mut sqlx::PgConnection,
    id: i64,
) -> Result<Option<Admin>, AppError> {
    let admin = sqlx::query_as!(
        Admin,
        r#"
                DELETE FROM admin 
                WHERE id = $1 AND role != 'SUPER_ADMIN'::ADMIN_ROLE
                RETURNING id, username, password_hash, role AS "role: DbAdminRole", created_at, updated_at
            "#,
        id
    )
    .fetch_optional(connection)
    .await.map_err(|error| AppError::from(error).at(concat!(module_path!(), "::delete")))?;

    Ok(admin)
}

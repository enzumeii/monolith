use crate::validations::admin::AdminPageRequest;
use crate::{error::AppError, repositories::admin, utils::password};
use domain::dto::{AdminDTO, AdminListResponse, CreateAdminRequest, UpdateAdminRequest};
use sqlx::PgPool;

#[tracing::instrument(skip(pool))]
pub async fn list(pool: &PgPool, page: AdminPageRequest) -> Result<AdminListResponse, AppError> {
    let mut entities = admin::list_page(pool, page).await?;
    let has_next = entities.len() > page.per_page() as usize;
    entities.truncate(page.per_page() as usize);
    let admins = entities.into_iter().map(AdminDTO::from).collect();

    Ok(AdminListResponse {
        admins,
        page: page.page(),
        per_page: page.per_page(),
        has_next,
    })
}

#[tracing::instrument(skip(pool, req), fields(username = %req.username, %req.role))]
pub async fn add(pool: &PgPool, req: CreateAdminRequest) -> Result<AdminDTO, AppError> {
    crate::validations::admin::validate_create(&req)?;
    let password_hash = password::hash(req.password).await?;
    let admin_entity = admin::add(pool, req.username, password_hash, req.role).await?;
    Ok(admin_entity.into())
}

#[tracing::instrument(skip(pool, req), fields(username = ?req.username))]
pub async fn update(pool: &PgPool, id: i64, req: UpdateAdminRequest) -> Result<AdminDTO, AppError> {
    crate::validations::admin::validate_update_input(&req)?;
    let password_hash = match req.password {
        Some(raw_password) => Some(password::hash(raw_password).await?),
        None => None,
    };
    let updated_entity = admin::update(pool, id, req.username, password_hash)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(updated_entity.into())
}

#[tracing::instrument(skip(pool))]
pub async fn get(pool: &PgPool, id: i64) -> Result<AdminDTO, AppError> {
    let admin_entity = admin::find_by_id(pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(admin_entity.into())
}

#[tracing::instrument(skip(pool))]
pub async fn delete(pool: &PgPool, id: i64) -> Result<AdminDTO, AppError> {
    let deleted_admin = admin::delete(pool, id).await?;
    match deleted_admin {
        Some(admin_entity) => Ok(admin_entity.into()),
        None if admin::find_by_id(pool, id).await?.is_some() => Err(AppError::Forbidden),
        None => Err(AppError::NotFound),
    }
}

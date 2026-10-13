use crate::{
    error::AppError,
    models::AuthenticatedAdmin,
    models::DbAdminRole,
    repositories::{admin, auth},
    utils::password,
    validations::admin::{self as validate, AdminPageRequest},
};
use domain::dto::{AdminDTO, AdminListResponse, CreateAdminRequest, UpdateAdminRequest};
use sqlx::PgPool;

pub async fn list(pool: &PgPool, page: AdminPageRequest) -> Result<AdminListResponse, AppError> {
    let mut rows = admin::list_page(pool, page).await?;
    let has_next = rows.len() > page.per_page() as usize;
    rows.truncate(page.per_page() as usize);
    Ok(AdminListResponse {
        admins: rows.into_iter().map(AdminDTO::from).collect(),
        page: page.page(),
        per_page: page.per_page(),
        has_next,
    })
}

pub async fn add(
    pool: &PgPool,
    actor: &AuthenticatedAdmin,
    req: CreateAdminRequest,
) -> Result<AdminDTO, AppError> {
    validate::validate_create(&req)?;
    let hash = password::hash(req.password).await?;
    let current = admin::find_by_id(pool, actor.id)
        .await?
        .ok_or(AppError::InvalidToken)?;
    if current.role != DbAdminRole::SuperAdmin {
        return Err(AppError::Forbidden);
    }
    let mut connection = pool.acquire().await?;
    let row = admin::add(&mut connection, req.username, hash, req.role).await?;
    Ok(row.into())
}

pub async fn update(
    pool: &PgPool,
    actor: &AuthenticatedAdmin,
    id: i64,
    req: UpdateAdminRequest,
) -> Result<AdminDTO, AppError> {
    validate::validate_update(actor, id)?;
    validate::validate_update_input(&req)?;
    let current = admin::find_by_id(pool, actor.id)
        .await?
        .ok_or(AppError::InvalidToken)?;
    validate::validate_update(&AuthenticatedAdmin::from(&current), id)?;
    let hash = match req.password {
        Some(value) => Some(password::hash(value).await?),
        None => None,
    };
    let mut tx = pool.begin().await?;

    let row = admin::update(&mut tx, id, req.username, hash)
        .await?
        .ok_or(AppError::NotFound)?;
    auth::delete_refresh_tokens(&mut tx, id).await?;
    tx.commit().await?;
    Ok(row.into())
}

pub async fn get(pool: &PgPool, id: i64) -> Result<AdminDTO, AppError> {
    Ok(admin::find_by_id(pool, id)
        .await?
        .ok_or(AppError::NotFound)?
        .into())
}

pub async fn delete(
    pool: &PgPool,
    actor: &AuthenticatedAdmin,
    id: i64,
) -> Result<AdminDTO, AppError> {
    validate::validate_delete(actor, id)?;
    let current = admin::find_by_id(pool, actor.id)
        .await?
        .ok_or(AppError::InvalidToken)?;
    let target = admin::find_by_id(pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    if current.role != DbAdminRole::SuperAdmin || target.role == DbAdminRole::SuperAdmin {
        return Err(AppError::Forbidden);
    }
    let mut connection = pool.acquire().await?;
    let row = admin::delete(&mut connection, id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(row.into())
}

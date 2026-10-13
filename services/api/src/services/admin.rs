use crate::{
    auth::AuthenticatedAdmin,
    error::AppError,
    models::Admin,
    repositories::{admin, auth},
    utils::password,
    validations::admin::{self as validate, AdminPageRequest},
};
use domain::dto::{AdminDTO, AdminListResponse, CreateAdminRequest, UpdateAdminRequest};
use sqlx::{PgConnection, PgPool};

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

async fn lock_actor_and_target(
    connection: &mut PgConnection,
    actor: &AuthenticatedAdmin,
    id: i64,
) -> Result<(AuthenticatedAdmin, Admin), AppError> {
    let mut ids = vec![actor.id, id];
    ids.sort_unstable();
    ids.dedup();
    let mut current_actor = None;
    let mut target = None;
    for account_id in ids {
        if let Some(account) = admin::lock(connection, account_id).await? {
            if account_id == actor.id {
                current_actor = Some(AuthenticatedAdmin::from(&account));
            }
            if account_id == id {
                target = Some(account);
            }
        }
    }
    let current = current_actor.ok_or(AppError::InvalidToken)?;
    Ok((current, target.ok_or(AppError::NotFound)?))
}

pub async fn add(
    pool: &PgPool,
    limiter: &password::PasswordLimiter,
    actor: &AuthenticatedAdmin,
    req: CreateAdminRequest,
) -> Result<AdminDTO, AppError> {
    validate::validate_create(&req)?;
    let hash = password::hash(limiter, req.password).await?;
    let mut tx = pool.begin().await?;
    let (current, _) = lock_actor_and_target(&mut tx, actor, actor.id).await?;
    if current.role != domain::AdminRole::SuperAdmin {
        return Err(AppError::Forbidden);
    }
    let row = admin::add(&mut tx, req.username, hash, req.role).await?;
    tx.commit().await?;
    Ok(row.into())
}

pub async fn update(
    pool: &PgPool,
    limiter: &password::PasswordLimiter,
    actor: &AuthenticatedAdmin,
    id: i64,
    req: UpdateAdminRequest,
) -> Result<AdminDTO, AppError> {
    validate::validate_update(actor, id)?;
    validate::validate_update_input(&req)?;
    let hash = match req.password {
        Some(value) => Some(password::hash(limiter, value).await?),
        None => None,
    };
    let mut tx = pool.begin().await?;
    let (current, _) = lock_actor_and_target(&mut tx, actor, id).await?;
    validate::validate_update(&current, id)?;
    let row = admin::update(&mut tx, id, req.username, hash)
        .await?
        .ok_or(AppError::NotFound)?;
    auth::revoke_account(&mut tx, id).await?;
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
    let mut tx = pool.begin().await?;
    let (current, target) = lock_actor_and_target(&mut tx, actor, id).await?;
    if current.role != domain::AdminRole::SuperAdmin
        || target.role == crate::models::DbAdminRole::SuperAdmin
    {
        return Err(AppError::Forbidden);
    }
    let row = admin::delete(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(row.into())
}

/// Explicit operator bootstrap; no HTTP endpoint invokes this function.
pub async fn bootstrap(
    pool: &PgPool,
    limiter: &password::PasswordLimiter,
    req: CreateAdminRequest,
) -> Result<AdminDTO, AppError> {
    validate::validate_create(&req)?;
    let hash = password::hash(limiter, req.password).await?;
    let mut tx = pool.begin().await?;
    let account = admin::add(&mut tx, req.username, hash, req.role).await?;
    tx.commit().await?;
    Ok(account.into())
}

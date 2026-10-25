use crate::{
    error::AppError, models::AdminPageRequest, models::AuthenticatedAdmin, models::DbAdminRole,
    repositories, transformers, utils::password,
};
use axum::http::StatusCode;
use domain::dto::{AdminDTO, AdminListResponse, AdminRole, CreateAdminRequest, UpdateAdminRequest};
use sqlx::PgPool;

pub async fn list(pool: &PgPool, page: AdminPageRequest) -> Result<AdminListResponse, AppError> {
    let mut rows = repositories::admin::list_page(pool, page).await?;
    let has_next = rows.len() > page.per_page() as usize;
    rows.truncate(page.per_page() as usize);
    Ok(AdminListResponse {
        admins: rows
            .into_iter()
            .map(transformers::admin::response)
            .collect(),
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
    let hash = password::hash(req.password).await?;
    let current = repositories::admin::find_by_id(pool, actor.id)
        .await?
        .ok_or_else(|| {
            AppError::new(StatusCode::UNAUTHORIZED, "Invalid or expired token")
                .with_bearer_challenge()
        })?;
    if current.role != DbAdminRole::SuperAdmin {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "Your account is forbidden to make this action",
        ));
    }
    let mut connection = pool.acquire().await?;
    let row = repositories::admin::add(&mut connection, req.username, hash, req.role).await?;
    Ok(transformers::admin::response(row))
}

pub async fn update(
    pool: &PgPool,
    actor: &AuthenticatedAdmin,
    id: i64,
    req: UpdateAdminRequest,
) -> Result<AdminDTO, AppError> {
    let current = repositories::admin::find_by_id(pool, actor.id)
        .await?
        .ok_or_else(|| {
            AppError::new(StatusCode::UNAUTHORIZED, "Invalid or expired token")
                .with_bearer_challenge()
        })?;
    authorize_update(&AuthenticatedAdmin::from(&current), id)?;
    let hash = match req.password {
        Some(value) => Some(password::hash(value).await?),
        None => None,
    };
    let mut tx = pool.begin().await?;

    let row = repositories::admin::update(&mut tx, id, req.username, hash)
        .await?
        .ok_or_else(|| {
            AppError::new(StatusCode::UNAUTHORIZED, "Not authorized").with_bearer_challenge()
        })?;
    repositories::auth::delete_account_refresh_tokens(&mut tx, id).await?;
    tx.commit().await?;
    Ok(transformers::admin::response(row))
}

pub async fn get(pool: &PgPool, id: i64) -> Result<AdminDTO, AppError> {
    Ok(transformers::admin::response(
        repositories::admin::find_by_id(pool, id)
            .await?
            .ok_or_else(|| {
                AppError::new(StatusCode::UNAUTHORIZED, "Not authorized").with_bearer_challenge()
            })?,
    ))
}

pub async fn delete(
    pool: &PgPool,
    actor: &AuthenticatedAdmin,
    id: i64,
) -> Result<AdminDTO, AppError> {
    authorize_delete(actor, id)?;
    let current = repositories::admin::find_by_id(pool, actor.id)
        .await?
        .ok_or_else(|| {
            AppError::new(StatusCode::UNAUTHORIZED, "Invalid or expired token")
                .with_bearer_challenge()
        })?;
    let target = repositories::admin::find_by_id(pool, id)
        .await?
        .ok_or_else(|| {
            AppError::new(StatusCode::UNAUTHORIZED, "Not authorized").with_bearer_challenge()
        })?;
    if current.role != DbAdminRole::SuperAdmin || target.role == DbAdminRole::SuperAdmin {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "Your account is forbidden to make this action",
        ));
    }
    let mut connection = pool.acquire().await?;
    let row = repositories::admin::delete(&mut connection, id)
        .await?
        .ok_or_else(|| {
            AppError::new(StatusCode::UNAUTHORIZED, "Not authorized").with_bearer_challenge()
        })?;
    Ok(transformers::admin::response(row))
}

pub fn authorize_update(claims: &AuthenticatedAdmin, target_id: i64) -> Result<(), AppError> {
    let allowed = claims.role == AdminRole::SuperAdmin || claims.id == target_id;
    if !allowed {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "Your account is forbidden to make this action",
        ));
    }
    Ok(())
}

fn authorize_delete(claims: &AuthenticatedAdmin, target_id: i64) -> Result<(), AppError> {
    if claims.id == target_id {
        return Err(AppError::new(
            StatusCode::FORBIDDEN,
            "Your account is forbidden to make this action",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/services/admin.rs"]
mod tests;

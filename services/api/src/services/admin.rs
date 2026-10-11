use crate::{error::AppError, repositories::admin};
use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use domain::dto::{
    AdminDTO, AdminListResponse, AdminPageRequest, CreateAdminRequest, UpdateAdminRequest,
};
use domain::entities::Admin;
use sqlx::PgPool;

//////////////////////// TESTS ////////////////////////

//////////////////////// PURE CORE ////////////////////////

pub fn build_list_response(
    admin_entities: Vec<Admin>,
    page: AdminPageRequest,
) -> AdminListResponse {
    let mut entities = admin_entities;
    let has_next = entities.len() > page.per_page as usize;
    entities.truncate(page.per_page as usize);
    let admins = entities.into_iter().map(AdminDTO::from).collect();

    AdminListResponse {
        admins,
        page: page.page,
        per_page: page.per_page,
        has_next,
    }
}

pub fn resolve_delete(deleted_admin: Option<Admin>, exists: bool) -> Result<AdminDTO, AppError> {
    let Some(admin_entity) = deleted_admin else {
        if exists {
            return Err(AppError::Forbidden);
        }
        return Err(AppError::NotFound);
    };
    Ok(admin_entity.into())
}

pub fn ensure_self(claims_id: i64, target_id: i64) -> Result<(), AppError> {
    if claims_id != target_id {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

//////////////////////// IMPERATIVE SHELL ////////////////////////

#[tracing::instrument(skip(pool))]
pub async fn list(pool: &PgPool, page: AdminPageRequest) -> Result<AdminListResponse, AppError> {
    let admin_entities = admin::list_page(pool, page).await?;
    Ok(build_list_response(admin_entities, page))
}

#[tracing::instrument(skip(pool, req), fields(username = %req.username, %req.role))]
pub async fn add(pool: &PgPool, req: CreateAdminRequest) -> Result<AdminDTO, AppError> {
    let password_hash = hash_password(req.password).await?;
    let admin_entity = admin::add(pool, req.username, password_hash, req.role).await?;
    Ok(admin_entity.into())
}

#[tracing::instrument(skip(pool, req), fields(username = ?req.username))]
pub async fn update(pool: &PgPool, id: i64, req: UpdateAdminRequest) -> Result<AdminDTO, AppError> {
    let password_hash = match req.password {
        Some(raw_password) => Some(hash_password(raw_password).await?),
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
    let exists = deleted_admin.is_none() && admin::find_by_id(pool, id).await?.is_some();
    resolve_delete(deleted_admin, exists)
}

async fn hash_password(password: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(AppError::from)
    })
    .await
    .map_err(|error| AppError::Internal(anyhow::Error::msg(error.to_string())))?
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use argon2::{PasswordHash, PasswordVerifier};
    use chrono::Utc;
    use domain::entities::AdminRole;

    fn admin(id: i64) -> Admin {
        Admin {
            id,
            username: format!("admin-{id}"),
            password_hash: "hash".into(),
            role: AdminRole::Admin,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn build_list_response_truncates_extra_record_and_sets_has_next() {
        let admins = (1..=26).map(admin).collect();
        let response = build_list_response(
            admins,
            AdminPageRequest {
                page: 1,
                per_page: 25,
            },
        );

        assert_eq!(response.admins.len(), 25);
        assert!(response.has_next);
        assert_eq!(response.admins[0].id, 1);
        assert_eq!(response.admins[24].id, 25);
    }

    #[test]
    fn build_list_response_without_extra_record_has_no_next_page() {
        let admins = (1..=25).map(admin).collect();
        let response = build_list_response(
            admins,
            AdminPageRequest {
                page: 1,
                per_page: 25,
            },
        );

        assert_eq!(response.admins.len(), 25);
        assert!(!response.has_next);
    }

    #[test]
    fn resolve_delete_returns_ok_when_deleted() {
        let admin = admin(1);
        let res = resolve_delete(Some(admin), true);
        assert!(res.is_ok());
        assert_eq!(res.unwrap().id, 1);
    }

    #[test]
    fn resolve_delete_returns_forbidden_when_not_deleted_but_exists() {
        let res = resolve_delete(None, true);
        assert!(matches!(res, Err(AppError::Forbidden)));
    }

    #[test]
    fn resolve_delete_returns_not_found_when_not_deleted_and_not_exists() {
        let res = resolve_delete(None, false);
        assert!(matches!(res, Err(AppError::NotFound)));
    }

    #[test]
    fn ensure_self_passes_when_ids_match() {
        assert!(ensure_self(1, 1).is_ok());
    }

    #[test]
    fn ensure_self_returns_forbidden_when_ids_mismatch() {
        assert!(matches!(ensure_self(1, 2), Err(AppError::Forbidden)));
    }

    #[tokio::test]
    async fn hash_password_produces_a_valid_verifiable_hash() {
        let hash = hash_password("my-password".into()).await.unwrap();
        assert_ne!(hash, "my-password");
        let parsed = PasswordHash::new(&hash).unwrap();
        assert!(
            Argon2::default()
                .verify_password(b"my-password", &parsed)
                .is_ok()
        );
        assert!(
            Argon2::default()
                .verify_password(b"wrong-password", &parsed)
                .is_err()
        );
    }
}

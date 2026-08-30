use crate::{error::AppError, repositories::admin::AdminRepositoryTrait};
use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use async_trait::async_trait;
use domain::dto::{
    AdminDTO, AdminListResponse, AdminPageRequest, CreateAdminRequest, UpdateAdminRequest,
};
use std::sync::Arc;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait AdminServiceTrait: Send + Sync {
    async fn list(&self, page: AdminPageRequest) -> Result<AdminListResponse, AppError>;

    async fn add(&self, req: CreateAdminRequest) -> Result<AdminDTO, AppError>;

    async fn update(&self, id: i64, req: UpdateAdminRequest) -> Result<AdminDTO, AppError>;

    async fn get(&self, id: i64) -> Result<AdminDTO, AppError>;

    async fn delete(&self, id: i64) -> Result<AdminDTO, AppError>;
}

#[derive(Debug)]
pub struct AdminService {
    admin_repo: Arc<dyn AdminRepositoryTrait>,
}

impl AdminService {
    pub fn new(admin_repo: Arc<dyn AdminRepositoryTrait>) -> Self {
        Self { admin_repo }
    }
}

#[async_trait]
impl AdminServiceTrait for AdminService {
    #[tracing::instrument(skip(self))]
    async fn list(&self, page: AdminPageRequest) -> Result<AdminListResponse, AppError> {
        let mut admin_entities = self.admin_repo.list_page(page).await?;
        let has_next = admin_entities.len() > page.per_page as usize;
        admin_entities.truncate(page.per_page as usize);
        let admins = admin_entities.into_iter().map(AdminDTO::from).collect();

        Ok(AdminListResponse {
            admins,
            page: page.page,
            per_page: page.per_page,
            has_next,
        })
    }

    #[tracing::instrument(skip(self, req), fields(username = %req.username, %req.role))]
    async fn add(&self, req: CreateAdminRequest) -> Result<AdminDTO, AppError> {
        let password_hash = hash_password(req.password).await?;

        let admin_entity = self
            .admin_repo
            .add(req.username, password_hash, req.role)
            .await?;
        let admin_dto: AdminDTO = admin_entity.into();
        Ok(admin_dto)
    }

    #[tracing::instrument(skip(self, req), fields(username = ?req.username))]
    async fn update(&self, id: i64, req: UpdateAdminRequest) -> Result<AdminDTO, AppError> {
        let password_hash = match req.password {
            Some(raw_password) => Some(hash_password(raw_password).await?),
            None => None,
        };

        let updated_entity = self
            .admin_repo
            .update(id, req.username, password_hash)
            .await?
            .ok_or(AppError::NotFound)?;

        let admin_dto: AdminDTO = updated_entity.into();
        Ok(admin_dto)
    }

    #[tracing::instrument(skip(self))]
    async fn get(&self, id: i64) -> Result<AdminDTO, AppError> {
        let admin_entity = self
            .admin_repo
            .find_by_id(id)
            .await?
            .ok_or(AppError::NotFound)?;

        let admin_dto: AdminDTO = admin_entity.into();
        Ok(admin_dto)
    }

    #[tracing::instrument(skip(self))]
    async fn delete(&self, id: i64) -> Result<AdminDTO, AppError> {
        let deleted_admin = self.admin_repo.delete(id).await?;

        let Some(admin_entity) = deleted_admin else {
            if self.admin_repo.find_by_id(id).await?.is_some() {
                return Err(AppError::Forbidden);
            }
            return Err(AppError::NotFound);
        };

        let admin_dto: AdminDTO = admin_entity.into();

        Ok(admin_dto)
    }
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
    use std::sync::Arc;

    use argon2::PasswordVerifier;
    use chrono::Utc;
    use domain::entities::{Admin, AdminRole};

    use super::*;
    use crate::repositories::admin::MockAdminRepositoryTrait;

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

    #[tokio::test]
    async fn list_page_truncates_extra_record_and_sets_has_next() {
        let mut repo = MockAdminRepositoryTrait::new();
        repo.expect_list_page()
            .withf(|page| page.page == 1 && page.per_page == 25)
            .returning(|_| Ok((1..=26).map(admin).collect()));
        let service = AdminService::new(Arc::new(repo));

        let response = service
            .list(AdminPageRequest {
                page: 1,
                per_page: 25,
            })
            .await
            .unwrap();

        assert_eq!(response.admins.len(), 25);
        assert!(response.has_next);
        assert_eq!(response.admins[0].id, 1);
        assert_eq!(response.admins[24].id, 25);
    }

    #[tokio::test]
    async fn add_hashes_password_before_persisting() {
        let mut repo = MockAdminRepositoryTrait::new();
        repo.expect_add()
            .withf(|username, hash, role| {
                username == "new-admin"
                    && *role == AdminRole::Admin
                    && hash != "password"
                    && Argon2::default()
                        .verify_password(b"password", &argon2::PasswordHash::new(hash).unwrap())
                        .is_ok()
            })
            .returning(|username, password_hash, role| {
                Ok(Admin {
                    id: 1,
                    username,
                    password_hash,
                    role,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                })
            });
        let service = AdminService::new(Arc::new(repo));

        let response = service
            .add(CreateAdminRequest {
                username: "new-admin".into(),
                password: "password".into(),
                role: AdminRole::Admin,
            })
            .await
            .unwrap();

        assert_eq!(response.username, "new-admin");
    }

    #[tokio::test]
    async fn list_page_without_extra_record_has_no_next_page() {
        let mut repo = MockAdminRepositoryTrait::new();
        repo.expect_list_page()
            .returning(|_| Ok((1..=25).map(admin).collect()));
        let service = AdminService::new(Arc::new(repo));

        let response = service
            .list(AdminPageRequest {
                page: 1,
                per_page: 25,
            })
            .await
            .unwrap();

        assert_eq!(response.admins.len(), 25);
        assert!(!response.has_next);
    }
}

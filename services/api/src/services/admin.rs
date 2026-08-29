use crate::{error::AppError, repositories::admin::AdminRepositoryTrait};
use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use async_trait::async_trait;
use domain::dto::{AdminDTO, AdminListResponse, CreateAdminRequest, UpdateAdminRequest};
use std::sync::Arc;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait AdminServiceTrait: Send + Sync {
    async fn list(&self) -> Result<AdminListResponse, AppError>;

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
    async fn list(&self) -> Result<AdminListResponse, AppError> {
        let admin_entities = self.admin_repo.list().await?;

        let admin_dtos: Vec<AdminDTO> = admin_entities.into_iter().map(AdminDTO::from).collect();

        Ok(AdminListResponse { admins: admin_dtos })
    }

    #[tracing::instrument(skip(self, req), fields(username = %req.username, %req.role))]
    async fn add(&self, req: CreateAdminRequest) -> Result<AdminDTO, AppError> {
        let salt = SaltString::generate(&mut OsRng);

        let password_hash = Argon2::default()
            .hash_password(req.password.as_bytes(), &salt)?
            .to_string();

        let admin_entity = self
            .admin_repo
            .add(req.username, password_hash, req.role)
            .await?;
        let admin_dto: AdminDTO = admin_entity.into();
        Ok(admin_dto)
    }

    #[tracing::instrument(skip(self, req), fields(username = ?req.username))]
    async fn update(&self, id: i64, req: UpdateAdminRequest) -> Result<AdminDTO, AppError> {
        let password_hash = if let Some(raw_password) = req.password {
            let salt = SaltString::generate(&mut OsRng);
            Some(
                Argon2::default()
                    .hash_password(raw_password.as_bytes(), &salt)?
                    .to_string(),
            )
        } else {
            None
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

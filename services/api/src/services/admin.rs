use argon2::{
    Argon2, PasswordHasher,
    password_hash::{SaltString, rand_core::OsRng},
};
use domain::dto::{AdminDTO, AdminListResponse, CreateAdminRequest, UpdateAdminRequest};
use sqlx::PgPool;

use crate::{error::AppError, repositories::admin::AdminRepository};

pub struct AdminService;
impl AdminService {
    #[tracing::instrument(skip(pool))]
    pub async fn list(pool: &PgPool) -> Result<AdminListResponse, AppError> {
        let admin_entities = AdminRepository::list(pool).await?;

        let admin_dtos: Vec<AdminDTO> = admin_entities.into_iter().map(AdminDTO::from).collect();

        Ok(AdminListResponse { admins: admin_dtos })
    }

    #[tracing::instrument(skip(pool, req), fields(username = %req.username, %req.role))]
    pub async fn add(pool: &PgPool, req: CreateAdminRequest) -> Result<AdminDTO, AppError> {
        let salt = SaltString::generate(&mut OsRng);

        let password_hash = Argon2::default()
            .hash_password(req.password.as_bytes(), &salt)?
            .to_string();

        let admin_entity =
            AdminRepository::add(pool, req.username, password_hash, req.role).await?;
        let admin_dto: AdminDTO = admin_entity.into();
        Ok(admin_dto)
    }

    #[tracing::instrument(skip(pool, req), fields(username = ?req.username))]
    pub async fn update(
        pool: &PgPool,
        id: i64,
        req: UpdateAdminRequest,
    ) -> Result<AdminDTO, AppError> {
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

        let updated_entity = AdminRepository::update(pool, id, req.username, password_hash)
            .await?
            .ok_or(AppError::NotFound)?;

        let admin_dto: AdminDTO = updated_entity.into();
        Ok(admin_dto)
    }

    #[tracing::instrument(skip(pool))]
    pub async fn get(pool: &PgPool, id: i64) -> Result<AdminDTO, AppError> {
        let admin_entity = AdminRepository::find_by_id(pool, id)
            .await?
            .ok_or(AppError::NotFound)?;

        let admin_dto: AdminDTO = admin_entity.into();
        Ok(admin_dto)
    }

    #[tracing::instrument(skip(pool))]
    pub async fn delete(pool: &PgPool, id: i64) -> Result<AdminDTO, AppError> {
        let deleted_admin = AdminRepository::delete(pool, id).await?;

        let Some(admin_entity) = deleted_admin else {
            if AdminRepository::find_by_id(pool, id).await?.is_some() {
                return Err(AppError::Forbidden);
            }
            return Err(AppError::NotFound);
        };

        let admin_dto: AdminDTO = admin_entity.into();

        Ok(admin_dto)
    }
}

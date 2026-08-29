use domain::dto::{AdminDTO, AdminListResponse};
use sqlx::PgPool;

use crate::{error::AppError, repositories::admin::AdminRepository};

pub struct AdminService;
impl AdminService {
    #[tracing::instrument(skip(pool))]
    pub async fn list_admins(pool: &PgPool) -> Result<AdminListResponse, AppError> {
        let admin_entities = AdminRepository::list_admins(pool).await?;

        let admins_dto: Vec<AdminDTO> = admin_entities.into_iter().map(AdminDTO::from).collect();

        Ok(AdminListResponse { admins: admins_dto })
    }
}

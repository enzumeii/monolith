use sqlx::PgPool;

use crate::{
    config::Settings,
    repositories::{
        admin::{AdminRepositoryTrait, PgAdminRepository},
        auth::{AuthRepositoryTrait, PgAuthRepository},
    },
    services::{
        admin::{AdminService, AdminServiceTrait},
        auth::{AuthService, AuthServiceTrait},
    },
};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub admin_service: Arc<dyn AdminServiceTrait>,
    pub auth_service: Arc<dyn AuthServiceTrait>,
}

impl AppState {
    #[must_use]
    pub fn bind(pool: &PgPool, settings: Settings) -> Self {
        /////////////////// REPOSITORIES ///////////////////
        let admin_repo: Arc<dyn AdminRepositoryTrait> =
            Arc::new(PgAdminRepository::new(pool.clone()));
        let auth_repo: Arc<dyn AuthRepositoryTrait> = Arc::new(PgAuthRepository::new(pool.clone()));

        /////////////////// SERVICES ///////////////////
        let admin_service = AdminService::new(Arc::clone(&admin_repo));
        let auth_service = AuthService::new(Arc::clone(&auth_repo), Arc::clone(&admin_repo));

        Self {
            settings: Arc::new(settings),
            admin_service: Arc::new(admin_service),
            auth_service: Arc::new(auth_service),
        }
    }
}

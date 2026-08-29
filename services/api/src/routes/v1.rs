mod admin;
mod admins;

use crate::state::AppState;
use axum::Router;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/admin", admin::router())
        .nest("/admins", admins::router(state))
}

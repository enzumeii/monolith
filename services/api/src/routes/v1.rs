mod admins;
mod auth;

use crate::state::AppState;
use axum::Router;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/auth", auth::router())
        .nest("/admins", admins::router(state))
}

mod admin;

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new().nest("/admin", admin::router())
}

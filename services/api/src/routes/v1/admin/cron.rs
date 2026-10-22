mod configs;
mod report;

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/configs", configs::router())
        .nest("/reports", report::router())
}

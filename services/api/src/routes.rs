mod healthz;
mod v1;

use crate::state::AppState;
use axum::Router;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(healthz::router())
        .nest("/v1", v1::router(state.clone()))
        .with_state(state)
}

use crate::{handlers::report_results, state::AppState};
use axum::{Router, routing::get};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(report_results::list))
        .route("/{id}", get(report_results::detail))
        .route("/{id}/rows", get(report_results::rows))
        .route("/{id}/download", get(report_results::download))
}

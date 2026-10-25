use crate::{handlers, state::AppState};
use axum::{Router, routing::get};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(handlers::conventions::list).post(handlers::conventions::create),
        )
        .route("/catalogue", get(handlers::catalogue::get))
        .route("/options", get(handlers::conventions::options))
        .route(
            "/{id}",
            get(handlers::conventions::get)
                .patch(handlers::conventions::patch)
                .delete(handlers::conventions::delete),
        )
        .route(
            "/{id}/variants",
            get(handlers::conventions::variants).put(handlers::conventions::replace),
        )
}

use crate::{
    handlers::{catalogue, conventions},
    state::AppState,
};
use axum::{Router, routing::get};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(conventions::list).post(conventions::create))
        .route("/catalogue", get(catalogue::get))
        .route("/options", get(conventions::options))
        .route(
            "/{id}",
            get(conventions::get)
                .patch(conventions::patch)
                .delete(conventions::delete),
        )
        .route(
            "/{id}/variants",
            get(conventions::variants).put(conventions::replace),
        )
}

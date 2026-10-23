mod cron;
mod inventories;
mod orders;

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/conventions/options",
            axum::routing::get(crate::handlers::conventions::options),
        )
        .route(
            "/conventions/{id}/logo",
            axum::routing::put(crate::handlers::conventions::upload_logo).layer(
                axum::extract::DefaultBodyLimit::max(crate::storage::convention_logo::MAX_BYTES),
            ),
        )
        .route(
            "/conventions",
            axum::routing::get(crate::handlers::conventions::list)
                .post(crate::handlers::conventions::create),
        )
        .route(
            "/conventions/{id}",
            axum::routing::get(crate::handlers::conventions::get)
                .patch(crate::handlers::conventions::patch)
                .delete(crate::handlers::conventions::delete),
        )
        .route(
            "/conventions/{id}/variants",
            axum::routing::get(crate::handlers::conventions::variants)
                .put(crate::handlers::conventions::replace),
        )
        .route(
            "/catalogue/products",
            axum::routing::get(crate::handlers::catalogue::get),
        )
        .nest("/inventories", inventories::router())
        .nest("/orders", orders::router())
        .nest("/cron", cron::router())
        .route(
            "/transactions",
            axum::routing::get(crate::handlers::accounting::report),
        )
}

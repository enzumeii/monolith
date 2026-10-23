mod conventions;
mod cron;
mod inventories;
mod orders;

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/files",
            axum::routing::post(crate::handlers::files::upload).layer(
                axum::extract::DefaultBodyLimit::max(crate::storage::convention_logo::MAX_BYTES),
            ),
        )
        .nest("/conventions", conventions::router())
        .nest("/inventories", inventories::router())
        .nest("/orders", orders::router())
        .nest("/cron", cron::router())
        .route(
            "/transactions",
            axum::routing::get(crate::handlers::accounting::report),
        )
}

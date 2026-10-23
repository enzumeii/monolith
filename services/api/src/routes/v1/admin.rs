mod cron;
mod inventories;
mod orders;

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
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

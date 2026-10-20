mod inventories;
mod orders;

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/inventories", inventories::router())
        .nest("/orders", orders::router())
        .route(
            "/transactions",
            axum::routing::get(crate::handlers::accounting::report),
        )
}

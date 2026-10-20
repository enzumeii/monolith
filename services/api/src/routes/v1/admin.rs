mod cron_configurations;
mod inventories;
mod orders;
mod report_results;

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/inventories", inventories::router())
        .nest("/orders", orders::router())
        .nest("/cron-configurations", cron_configurations::router())
        .nest("/report-results", report_results::router())
        .route(
            "/transactions",
            axum::routing::get(crate::handlers::accounting::report),
        )
}

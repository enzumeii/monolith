mod admins;
mod auth;
mod inventories;
mod orders;
use crate::{middleware, state::AppState};
use axum::{Router, middleware as axum_middleware};

pub fn router(state: AppState) -> Router<AppState> {
    let public_routes = Router::new().nest("/auth", auth::public_router());

    let super_admin_routes = Router::new()
        .nest("/admins", admins::super_admin_router())
        .layer(axum_middleware::from_fn(
            middleware::token::require_super_admin,
        ));
    let protected_routes = Router::new()
        .nest("/auth", auth::authed_router())
        .nest("/admins", admins::router())
        .nest("/inventories", inventories::router())
        .nest("/orders", orders::router())
        .merge(super_admin_routes)
        .layer(axum_middleware::from_fn_with_state(
            state,
            middleware::token::auth_middleware,
        ));

    public_routes.merge(protected_routes)
}

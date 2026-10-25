use crate::{handlers, state::AppState};
use axum::{
    Router,
    extract::Request,
    http::{HeaderValue, header::CACHE_CONTROL},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::public_conventions::list))
        .route("/{slug}", get(handlers::public_conventions::get))
        .route(
            "/{slug}/products",
            get(handlers::public_conventions::products),
        )
        .route("/{slug}/quote", post(handlers::public_conventions::quote))
        .layer(middleware::from_fn(no_store))
}

async fn no_store(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#![cfg(feature = "server")]
use dioxus::prelude::*;
use domain::dto::{RefreshRequest, TokenResponse};
use futures::future::{BoxFuture, FutureExt, Shared};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

type Flight = Shared<BoxFuture<'static, ServerFnResult<TokenResponse>>>;
type Flights = Mutex<HashMap<[u8; 32], (Instant, Flight)>>;
#[derive(Clone, Default)]
pub(crate) struct RefreshCoordinator {
    flights: Arc<Flights>,
}
impl RefreshCoordinator {
    pub(crate) async fn rotate(
        &self,
        http: reqwest::Client,
        url: String,
        token: String,
    ) -> ServerFnResult<TokenResponse> {
        let hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let future = {
            let mut entries = self.flights.lock().await;
            entries.retain(|_, (start, _)| start.elapsed() < Duration::from_secs(40));
            if let Some((_, flight)) = entries.get(&hash) {
                flight.clone()
            } else {
                if entries.len() >= 128 {
                    return Err(crate::api::client::unavailable());
                }
                let weak: Weak<Flights> = Arc::downgrade(&self.flights);
                let started = Instant::now();
                let future = async move {
                    let result = async {
                        let response = http
                            .post(url)
                            .json(&RefreshRequest {
                                refresh_token: token,
                            })
                            .send()
                            .await
                            .map_err(|_| crate::api::client::unavailable())?;
                        crate::api::client::decode(response, "refresh").await
                    }
                    .await;
                    if let Some(map) = weak.upgrade() {
                        let mut map = map.lock().await;
                        if map.get(&hash).is_some_and(|(time, _)| *time == started) {
                            map.remove(&hash);
                        }
                    }
                    result
                }
                .boxed()
                .shared();
                entries.insert(hash, (started, future.clone()));
                future
            }
        };
        future.await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus_server::axum::{Json, Router, routing::post};
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[tokio::test]
    async fn overlapping_bff_refresh_requests_share_one_upstream_call() {
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let router = Router::new().route(
            "/refresh",
            post(move || {
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Json(TokenResponse {
                        access_token: "new-access".into(),
                        refresh_token: "b".repeat(64),
                        token_type: "Bearer".into(),
                        expires_in: 600,
                        id: 7,
                    })
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let url = format!("http://{}/refresh", listener.local_addr().expect("address"));
        let task = tokio::spawn(async move {
            dioxus_server::axum::serve(listener, router)
                .await
                .expect("upstream");
        });
        let coordinator = RefreshCoordinator::default();
        let http = reqwest::Client::new();
        let token = "a".repeat(64);
        let (first, second) = tokio::join!(
            coordinator.rotate(http.clone(), url.clone(), token.clone()),
            coordinator.rotate(http.clone(), url.clone(), token.clone())
        );
        assert_eq!(first.expect("first").access_token, "new-access");
        assert_eq!(second.expect("second").access_token, "new-access");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        coordinator
            .rotate(http, url, token)
            .await
            .expect("later request");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "completed credentials must not be cached for replay"
        );
        task.abort();
    }
    #[tokio::test]
    async fn cancelling_first_waiter_preserves_rotation_for_another_waiter() {
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let started = Arc::new(tokio::sync::Notify::new());
        let start = started.clone();
        let release = Arc::new(tokio::sync::Notify::new());
        let finish = release.clone();
        let router = Router::new().route(
            "/refresh",
            post(move || {
                let count = count.clone();
                let start = start.clone();
                let finish = finish.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    start.notify_one();
                    finish.notified().await;
                    Json(TokenResponse {
                        access_token: "new-access".into(),
                        refresh_token: "b".repeat(64),
                        token_type: "Bearer".into(),
                        expires_in: 600,
                        id: 7,
                    })
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let url = format!("http://{}/refresh", listener.local_addr().expect("address"));
        let task = tokio::spawn(async move {
            dioxus_server::axum::serve(listener, router)
                .await
                .expect("upstream");
        });
        let coordinator = RefreshCoordinator::default();
        let worker = coordinator.clone();
        let first_url = url.clone();
        let first = tokio::spawn(async move {
            worker
                .rotate(reqwest::Client::new(), first_url, "a".repeat(64))
                .await
        });
        started.notified().await;
        first.abort();
        let _ = first.await;
        release.notify_one();
        let response = coordinator
            .rotate(reqwest::Client::new(), url, "a".repeat(64))
            .await
            .expect("remaining waiter");
        assert_eq!(response.access_token, "new-access");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        task.abort();
    }
}

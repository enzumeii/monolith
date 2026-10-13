#![cfg(feature = "server")]

use std::sync::Arc;

use crate::config::Settings;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub http: reqwest::Client,
    pub(crate) refresh: crate::session::refresh::RefreshCoordinator,
}

impl AppState {
    pub fn new(settings: Settings) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build upstream HTTP client");
        Self {
            settings: Arc::new(settings),
            http,
            refresh: crate::session::refresh::RefreshCoordinator::default(),
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.settings.upstream.api_base, path)
    }
}

pub fn app_state() -> AppState {
    dioxus::fullstack::FullstackContext::current()
        .and_then(|ctx| ctx.extension::<AppState>())
        .expect("AppState must be registered as an Axum Extension")
}

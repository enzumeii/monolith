use dioxus::prelude::*;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ApiConfig {
    pub base_url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Settings {
    pub api: ApiConfig,
}

impl Settings {
    pub async fn new() -> Result<Self, String> {
        let file = asset!(
            "/application.toml",
            AssetOptions::builder().with_hash_suffix(false)
        );
        let response = reqwest::get(file.to_string())
            .await
            .map_err(|_| "Unable to load application.toml".to_string())?
            .error_for_status()
            .map_err(|_| "Unable to load application.toml".to_string())?;
        let contents = response
            .text()
            .await
            .map_err(|_| "Unable to read application.toml".to_string())?;
        toml::from_str(&contents).map_err(|error| format!("Invalid application.toml: {error}"))
    }
}

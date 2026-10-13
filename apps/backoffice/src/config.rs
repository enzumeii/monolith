use dioxus::prelude::*;
use serde::Deserialize;

const CONFIG_ASSET: Asset = asset!(
    "/application.toml",
    AssetOptions::builder().with_hash_suffix(false)
);

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
        let mut page = document::eval("dioxus.send(window.location.href);");
        let page_url = page
            .recv::<String>()
            .await
            .map_err(|_| "Unable to determine the application URL".to_string())?;
        let url = resolve_config_url(&CONFIG_ASSET.to_string(), &page_url)?;
        let response = reqwest::get(url)
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

fn resolve_config_url(asset_path: &str, page_url: &str) -> Result<reqwest::Url, String> {
    reqwest::Url::parse(page_url)
        .and_then(|page| page.join(asset_path))
        .map_err(|_| "Invalid application configuration URL".to_string())
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;

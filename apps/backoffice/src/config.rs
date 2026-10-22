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
        let mut settings: Self = toml::from_str(&contents)
            .map_err(|error| format!("Invalid application.toml: {error}"))?;
        settings.api.base_url = resolve_api_url(&settings.api.base_url, &page_url)?;
        Ok(settings)
    }
}

fn resolve_api_url(base: &str, page_url: &str) -> Result<String, String> {
    let page = reqwest::Url::parse(page_url).map_err(|_| "Invalid application URL")?;
    let url = if base.starts_with('/') && !base.starts_with("//") {
        page.join(base)
    } else {
        reqwest::Url::parse(base)
    }
    .map_err(|_| "Invalid API URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Invalid API URL".into());
    }
    Ok(url.to_string().trim_end_matches('/').to_owned())
}

fn resolve_config_url(asset_path: &str, page_url: &str) -> Result<reqwest::Url, String> {
    reqwest::Url::parse(page_url)
        .and_then(|page| page.join(asset_path))
        .map_err(|_| "Invalid application configuration URL".to_string())
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;

use crate::api::{self, ApiError, ApiResult};
use dioxus::prelude::*;
use reqwest::StatusCode;
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
pub struct VisitorConfig {
    pub country_lookup_url: String,
    pub timeout_ms: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ContactConfig {
    pub email: String,
    pub facebook_url: String,
    pub instagram_url: String,
    pub x_url: String,
    pub tiktok_url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Settings {
    pub api: ApiConfig,
    pub contact: ContactConfig,
    pub visitor: VisitorConfig,
}

impl Settings {
    pub async fn new() -> ApiResult<Self> {
        let (page_url, url) = {
            let _loading = ui::loading_guard();
            let mut page = document::eval("dioxus.send(window.location.href);");
            let page_url = page
                .recv::<String>()
                .await
                .map_err(|_| invalid("Unable to determine the application URL"))?;
            let url = resolve_config_url(&CONFIG_ASSET.to_string(), &page_url).map_err(invalid)?;
            (page_url, url)
        };
        api::retry(|| Self::load(url.clone(), &page_url)).await
    }

    async fn load(url: reqwest::Url, page_url: &str) -> ApiResult<Self> {
        let _loading = ui::loading_guard();
        let response = reqwest::get(url)
            .await
            .map_err(|error| ApiError::transport(&error, "Unable to load application.toml"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(ApiError::http(status, "Unable to load application.toml"));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|error| ApiError::transport(&error, "Unable to read application.toml"))?;
        let contents = std::str::from_utf8(&bytes)
            .map_err(|_| invalid("Invalid application.toml: contents are not UTF-8"))?;
        Self::parse(contents, page_url)
    }

    fn parse(contents: &str, page_url: &str) -> ApiResult<Self> {
        let mut settings: Self = toml::from_str(contents)
            .map_err(|error| invalid(format!("Invalid application.toml: {error}")))?;
        settings.api.base_url =
            resolve_api_url(&settings.api.base_url, page_url).map_err(invalid)?;
        settings.contact.validate().map_err(invalid)?;
        settings.visitor.validate().map_err(invalid)?;
        Ok(settings)
    }
}

impl VisitorConfig {
    fn validate(&self) -> Result<(), String> {
        let url = reqwest::Url::parse(&self.country_lookup_url)
            .map_err(|_| "Invalid country lookup URL")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || self.country_lookup_url.trim() != self.country_lookup_url
            || self.country_lookup_url.chars().any(char::is_control)
        {
            return Err("Invalid country lookup URL".into());
        }
        if !(1..=10_000).contains(&self.timeout_ms) {
            return Err("Country lookup timeout must be between 1 and 10000 milliseconds".into());
        }
        Ok(())
    }
}

impl ContactConfig {
    fn validate(&self) -> Result<(), String> {
        let (local, domain) = self.email.split_once('@').ok_or("Invalid contact email")?;
        if local.is_empty()
            || domain.is_empty()
            || domain.contains('@')
            || self
                .email
                .bytes()
                .any(|byte| !byte.is_ascii_graphic() || b"<>\"?#%/\\:&".contains(&byte))
        {
            return Err("Invalid contact email".into());
        }
        for (name, value) in [
            ("Facebook", &self.facebook_url),
            ("Instagram", &self.instagram_url),
            ("X", &self.x_url),
            ("TikTok", &self.tiktok_url),
        ] {
            let url =
                reqwest::Url::parse(value).map_err(|_| format!("Invalid contact {name} URL"))?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || value.trim() != value
                || value.chars().any(char::is_control)
            {
                return Err(format!("Invalid contact {name} URL"));
            }
        }
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, message)
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
pub(crate) mod tests;

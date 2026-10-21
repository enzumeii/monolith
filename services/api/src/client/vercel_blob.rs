use super::{ClientError, ClientErrorKind, HttpClient, check_status};
use crate::config::VercelBlobConfig;
use reqwest::{
    Url,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::Deserialize;
use std::time::Duration;

pub struct VercelBlobClient {
    http: HttpClient,
    endpoint: Url,
    headers: HeaderMap,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Blob {
    pub pathname: String,
    pub content_type: String,
    pub url: String,
    pub download_url: String,
}

impl VercelBlobClient {
    pub fn new(config: &VercelBlobConfig) -> Result<Self, ClientError> {
        let endpoint = Url::parse(&config.api_url).map_err(|_| invalid("invalid Blob API URL"))?;
        if !safe_url(&endpoint) || endpoint.query().is_some() {
            return Err(invalid("invalid Blob API URL"));
        }
        let token = config
            .token
            .as_deref()
            .filter(|token| !token.trim().is_empty())
            .ok_or_else(|| invalid("Blob token is missing"))?;
        let mut authorization = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| invalid("invalid Blob token"))?;
        authorization.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, authorization);
        headers.insert("x-api-version", HeaderValue::from_static("13"));
        Ok(Self {
            http: HttpClient::new(Duration::from_secs(config.timeout_secs))?,
            endpoint,
            headers,
        })
    }

    pub async fn put(
        &self,
        pathname: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<Blob, ClientError> {
        let mut url = self.endpoint.clone();
        url.query_pairs_mut().append_pair("pathname", pathname);
        let response = self
            .http
            .inner
            .put(url)
            .headers(self.headers.clone())
            .header("x-vercel-blob-access", "public")
            .header("x-content-type", content_type)
            .header("x-add-random-suffix", "0")
            .header("x-allow-overwrite", "0")
            .body(bytes)
            .send()
            .await?;
        check_status(&response)?;
        let blob: Blob = serde_json::from_slice(&bounded_body(response, 65536).await?)?;
        self.validate(&blob, pathname, content_type)?;
        Ok(blob)
    }

    pub async fn metadata(&self, pathname: &str) -> Result<Option<Blob>, ClientError> {
        let mut url = self.endpoint.clone();
        url.query_pairs_mut().append_pair("url", pathname);
        let response = self
            .http
            .inner
            .get(url)
            .headers(self.headers.clone())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        check_status(&response)?;
        let blob: Blob = serde_json::from_slice(&bounded_body(response, 65536).await?)?;
        self.validate(&blob, pathname, &blob.content_type)?;
        Ok(Some(blob))
    }

    pub async fn download(&self, url: &str, max_bytes: usize) -> Result<Vec<u8>, ClientError> {
        let url = self.download_url(url)?;
        let response = self.http.inner.get(url).send().await?;
        check_status(&response)?;
        bounded_body(response, max_bytes).await
    }

    pub fn validate(
        &self,
        blob: &Blob,
        pathname: &str,
        content_type: &str,
    ) -> Result<(), ClientError> {
        if blob.pathname != pathname || blob.content_type != content_type {
            return Err(protocol("Blob pathname or content type does not match"));
        }
        for value in [&blob.url, &blob.download_url] {
            let url = self.download_url(value)?;
            if url.path().trim_start_matches('/') != pathname {
                return Err(protocol("Blob URL pathname does not match"));
            }
        }
        Ok(())
    }

    fn download_url(&self, value: &str) -> Result<Url, ClientError> {
        let url = Url::parse(value).map_err(|_| protocol("invalid Blob URL"))?;
        let production = self.endpoint.as_str() == "https://vercel.com/api/blob";
        let trusted = if production {
            url.scheme() == "https"
                && url
                    .host_str()
                    .is_some_and(|host| host.ends_with(".public.blob.vercel-storage.com"))
                && url.port().is_none()
        } else {
            url.origin() == self.endpoint.origin()
        };
        if !safe_url(&url) || !trusted {
            return Err(protocol("untrusted Blob download origin"));
        }
        Ok(url)
    }
}
async fn bounded_body(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, ClientError> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(protocol("Blob response exceeds expected size"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > max_bytes.saturating_sub(bytes.len()) {
            return Err(protocol("Blob response exceeds expected size"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
fn safe_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}
fn invalid(message: &'static str) -> ClientError {
    ClientError::new(ClientErrorKind::InvalidInput(message))
}
fn protocol(message: &'static str) -> ClientError {
    ClientError::new(ClientErrorKind::Protocol(message))
}

#[cfg(test)]
#[path = "../tests/client/vercel_blob.rs"]
mod tests;

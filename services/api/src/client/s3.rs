use super::{ClientError, ClientErrorKind};
use crate::config::S3Config;
use aws_sdk_s3::{
    Client,
    config::{
        BehaviorVersion, Credentials, Region, RequestChecksumCalculation,
        ResponseChecksumValidation, retry::RetryConfig, timeout::TimeoutConfig,
    },
    error::SdkError,
    presigning::PresigningConfig,
    primitives::ByteStream,
};
use chrono::Timelike;
use std::time::Duration;

pub struct S3Client {
    inner: Client,
    bucket: String,
    timeout: Duration,
    signed_url_ttl_secs: u32,
    endpoint: reqwest::Url,
    force_path_style: bool,
}

#[derive(Clone, Debug)]
pub struct StorageObject {
    pub storage_key: String,
    pub content_type: String,
    pub size: u64,
}

pub struct PresignedDownload {
    pub url: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl S3Client {
    pub fn new(config: &S3Config) -> Result<Self, ClientError> {
        validate_config(config)?;
        let access = credential(config.access_key_id.as_deref())?;
        let secret = credential(config.secret_access_key.as_deref())?;
        let timeout = Duration::from_secs(config.timeout_secs);
        let endpoint = if config.endpoint.ends_with('/') {
            config.endpoint.clone()
        } else {
            format!("{}/", config.endpoint)
        };
        let sdk = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .credentials_provider(Credentials::new(
                access,
                secret,
                None,
                None,
                "configured-s3",
            ))
            .region(Region::new(config.region.clone()))
            .endpoint_url(&endpoint)
            .force_path_style(config.force_path_style)
            .retry_config(RetryConfig::standard().with_max_attempts(1))
            .timeout_config(
                TimeoutConfig::builder()
                    .connect_timeout(timeout)
                    .read_timeout(timeout)
                    .operation_timeout(timeout)
                    .operation_attempt_timeout(timeout)
                    .build(),
            )
            .request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
            .response_checksum_validation(ResponseChecksumValidation::WhenRequired)
            .build();
        Ok(Self {
            inner: Client::from_conf(sdk),
            bucket: config.bucket.clone(),
            timeout,
            signed_url_ttl_secs: config.signed_url_ttl_secs,
            endpoint: reqwest::Url::parse(&endpoint).map_err(|_| invalid("invalid S3 endpoint"))?,
            force_path_style: config.force_path_style,
        })
    }

    #[must_use]
    pub fn bucket(&self) -> &str {
        &self.bucket
    }

    pub async fn metadata(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<Option<StorageObject>, ClientError> {
        validate_bucket(bucket)?;
        validate_key(key)?;
        let response = match self
            .inner
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error)
                if error
                    .raw_response()
                    .is_some_and(|r| r.status().as_u16() == 404) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(sdk_error(error)),
        };
        let content_type = response
            .content_type
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| protocol("S3 content type is missing"))?;
        let size = response
            .content_length
            .and_then(|value| u64::try_from(value).ok())
            .ok_or_else(|| protocol("S3 content length is missing or invalid"))?;
        Ok(Some(StorageObject {
            storage_key: key.to_owned(),
            content_type,
            size,
        }))
    }

    pub async fn put(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<(), ClientError> {
        validate_bucket(bucket)?;
        validate_key(key)?;
        if content_type.is_empty() || content_type.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(invalid("invalid S3 content type"));
        }
        self.inner
            .put_object()
            .bucket(bucket)
            .key(key)
            .content_type(content_type)
            .body(ByteStream::from(bytes))
            .send()
            .await
            .map_err(sdk_error)?;
        Ok(())
    }

    pub async fn download(
        &self,
        bucket: &str,
        key: &str,
        max_bytes: usize,
    ) -> Result<Vec<u8>, ClientError> {
        validate_bucket(bucket)?;
        validate_key(key)?;
        tokio::time::timeout(self.timeout, async {
            let response = self
                .inner
                .get_object()
                .bucket(bucket)
                .key(key)
                .send()
                .await
                .map_err(sdk_error)?;
            if response.content_length.is_some_and(|length| {
                length < 0 || u64::try_from(length).map_or(true, |length| length > max_bytes as u64)
            }) {
                return Err(protocol("S3 response exceeds expected size"));
            }
            let mut body = response.body;
            let mut bytes = Vec::new();
            while let Some(chunk) = body.next().await {
                let chunk = chunk.map_err(|source| {
                    ClientError::new(ClientErrorKind::Upstream {
                        source: Box::new(source),
                        timed_out: false,
                    })
                })?;
                if chunk.len() > max_bytes.saturating_sub(bytes.len()) {
                    return Err(protocol("S3 response exceeds expected size"));
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(bytes)
        })
        .await
        .map_err(|source| {
            ClientError::new(ClientErrorKind::Upstream {
                source: Box::new(source),
                timed_out: true,
            })
        })?
    }

    pub async fn presign_download(
        &self,
        bucket: &str,
        key: &str,
        filename: &str,
        ttl_secs: u32,
    ) -> Result<PresignedDownload, ClientError> {
        self.presign_download_at(
            bucket,
            key,
            filename,
            ttl_secs,
            std::time::SystemTime::now(),
        )
        .await
    }

    pub async fn presign_image(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
    ) -> Result<PresignedDownload, ClientError> {
        if !matches!(content_type, "image/png" | "image/jpeg") {
            return Err(invalid("invalid image content type"));
        }
        self.presign_at(
            bucket,
            key,
            "inline",
            Some(content_type),
            self.signed_url_ttl_secs,
            std::time::SystemTime::now(),
        )
        .await
    }

    async fn presign_download_at(
        &self,
        bucket: &str,
        key: &str,
        filename: &str,
        ttl_secs: u32,
        signing_time: std::time::SystemTime,
    ) -> Result<PresignedDownload, ClientError> {
        validate_bucket(bucket)?;
        validate_key(key)?;
        if ttl_secs == 0 || ttl_secs > self.signed_url_ttl_secs {
            return Err(invalid("invalid S3 signed URL lifetime"));
        }
        if filename.len() > 255
            || std::path::Path::new(filename)
                .extension()
                .is_none_or(|extension| extension != "csv")
            || filename.starts_with('.')
            || !filename
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err(invalid("invalid CSV attachment filename"));
        }
        self.presign_at(
            bucket,
            key,
            &format!("attachment; filename=\"{filename}\""),
            None,
            ttl_secs,
            signing_time,
        )
        .await
    }

    async fn presign_at(
        &self,
        bucket: &str,
        key: &str,
        disposition: &str,
        content_type: Option<&str>,
        ttl_secs: u32,
        signing_time: std::time::SystemTime,
    ) -> Result<PresignedDownload, ClientError> {
        validate_bucket(bucket)?;
        validate_key(key)?;
        if ttl_secs == 0 || ttl_secs > self.signed_url_ttl_secs {
            return Err(invalid("invalid S3 signed URL lifetime"));
        }
        let presigning = PresigningConfig::builder()
            .start_time(signing_time)
            .expires_in(Duration::from_secs(u64::from(ttl_secs)))
            .build()
            .map_err(|_| invalid("invalid S3 signed URL lifetime"))?;
        let request = self
            .inner
            .get_object()
            .bucket(bucket)
            .key(key)
            .response_content_disposition(disposition)
            .set_response_content_type(content_type.map(str::to_owned))
            .presigned(presigning)
            .await
            .map_err(sdk_error)?;
        let url =
            reqwest::Url::parse(request.uri()).map_err(|_| protocol("invalid S3 signed URL"))?;
        let mut expected = self.endpoint.clone();
        let prefix = expected.path().trim_end_matches('/').to_owned();
        let key = encode_key(key);
        if self.force_path_style || url.origin() == self.endpoint.origin() {
            expected.set_path(&format!("{prefix}/{bucket}/{key}"));
        } else {
            let host = format!(
                "{}.{}",
                bucket,
                expected
                    .host_str()
                    .ok_or_else(|| protocol("invalid S3 signed URL"))?
            );
            expected
                .set_host(Some(&host))
                .map_err(|_| protocol("invalid S3 signed URL"))?;
            expected.set_path(&format!("{prefix}/{key}"));
        }
        if url.origin() != expected.origin()
            || url.path() != expected.path()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(protocol("invalid S3 signed URL destination"));
        }
        let expires_at = chrono::DateTime::<chrono::Utc>::from(signing_time)
            .with_nanosecond(0)
            .ok_or_else(|| protocol("invalid S3 signing time"))?
            + chrono::Duration::seconds(i64::from(ttl_secs));
        Ok(PresignedDownload {
            url: url.into(),
            expires_at,
        })
    }
}

fn encode_key(key: &str) -> String {
    use std::fmt::Write;
    let mut encoded = String::new();
    for byte in key.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            write!(encoded, "%{byte:02X}").expect("writing to a string cannot fail");
        }
    }
    encoded
}

fn validate_config(config: &S3Config) -> Result<(), ClientError> {
    let url = reqwest::Url::parse(&config.endpoint).map_err(|_| invalid("invalid S3 endpoint"))?;
    let loopback = matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    );
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
    {
        return Err(invalid("invalid S3 endpoint"));
    }
    if !(1..=300).contains(&config.timeout_secs) || !(1..=900).contains(&config.signed_url_ttl_secs)
    {
        return Err(invalid(
            "S3 deadlines and signed URL lifetime must be bounded and positive",
        ));
    }
    if config.region.is_empty()
        || !config
            .region
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(invalid("invalid S3 region"));
    }
    validate_bucket(&config.bucket)
}

fn validate_bucket(bucket: &str) -> Result<(), ClientError> {
    if !(3..=63).contains(&bucket.len())
        || bucket.contains("..")
        || !bucket.as_bytes()[0].is_ascii_alphanumeric()
        || !bucket.as_bytes()[bucket.len() - 1].is_ascii_alphanumeric()
        || !bucket.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
        || bucket.parse::<std::net::Ipv4Addr>().is_ok()
    {
        return Err(invalid("invalid S3 bucket"));
    }
    Ok(())
}

fn credential(value: Option<&str>) -> Result<&str, ClientError> {
    value
        .filter(|value| {
            !value.trim().is_empty() && value.bytes().all(|byte| byte.is_ascii_graphic())
        })
        .ok_or_else(|| invalid("S3 credentials are missing or invalid"))
}

fn validate_key(key: &str) -> Result<(), ClientError> {
    if key.is_empty()
        || key.len() > 1024
        || key.starts_with('/')
        || key.bytes().any(|byte| byte.is_ascii_control())
        || key.split('/').any(|part| matches!(part, "." | ".."))
    {
        return Err(invalid("invalid S3 object key"));
    }
    Ok(())
}

fn sdk_error<E: std::error::Error + Send + Sync + 'static>(source: SdkError<E>) -> ClientError {
    let timed_out = matches!(&source, SdkError::TimeoutError(_))
        || matches!(&source, SdkError::DispatchFailure(context) if context.is_timeout());
    ClientError::new(ClientErrorKind::Upstream {
        source: Box::new(source),
        timed_out,
    })
}

fn invalid(message: &'static str) -> ClientError {
    ClientError::new(ClientErrorKind::InvalidInput(message))
}

fn protocol(message: &'static str) -> ClientError {
    ClientError::new(ClientErrorKind::Protocol(message))
}

#[cfg(test)]
#[path = "../tests/client/s3.rs"]
mod tests;

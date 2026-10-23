use super::ApiResult;
use domain::dto::{UploadFileQuery, UploadedFileDTO};
use reqwest::Method;

pub async fn upload(
    location: &str,
    content_type: &str,
    bytes: Vec<u8>,
) -> ApiResult<UploadedFileDTO> {
    super::decode(
        super::request(Method::POST, "/admin/files")?
            .query(&UploadFileQuery {
                location: location.into(),
            })
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(bytes),
    )
    .await
}

#[cfg(test)]
#[path = "../tests/api/files.rs"]
mod tests;

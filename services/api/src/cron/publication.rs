use crate::{
    client::s3::S3Client,
    error::AppError,
    models::cron::{
        checkpoint::{ExecutionCheckpoint, FileCheckpoint},
        report::SavedFile,
    },
};
use axum::http::StatusCode;
use sha2::{Digest, Sha256};

pub(crate) struct GeneratedFile {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}
pub(crate) struct PreparedReport {
    pub checkpoint: ExecutionCheckpoint,
    pub files: Vec<GeneratedFile>,
}
pub(crate) async fn obtain(
    client: &S3Client,
    entry: &FileCheckpoint,
    original: Option<&[u8]>,
) -> Result<(SavedFile, Vec<u8>), AppError> {
    entry.validate()?;
    if entry
        .receipt
        .as_ref()
        .is_some_and(|receipt| receipt.bucket != client.bucket())
        || original
            .is_some_and(|bytes| bytes.len() != entry.size_bytes || digest(bytes) != entry.sha256)
    {
        return Err(unavailable());
    }
    let object = if let Some(object) = client
        .metadata(&entry.key)
        .await
        .map_err(|e| crate::error::upstream(e))?
    {
        object
    } else {
        let bytes = original
            .filter(|_| entry.receipt.is_none())
            .ok_or_else(|| {
                AppError::new(
                    StatusCode::CONFLICT,
                    "Generated report file is missing from S3; regeneration is disabled",
                )
            })?;
        let outcome = client
            .put(&entry.key, &entry.content_type, bytes.to_vec())
            .await;
        // An accepted PUT can lose its response. Reconcile by key without replaying the upload.
        let object = client
            .metadata(&entry.key)
            .await
            .map_err(|e| crate::error::upstream(e))?;
        match (object, outcome) {
            (Some(object), _) => object,
            (None, Err(error)) => return Err(crate::error::upstream(error)),
            (None, Ok(())) => return Err(unavailable()),
        }
    };
    if object.storage_key != entry.key
        || object.content_type != entry.content_type
        || object.size != entry.size_bytes as u64
    {
        return Err(unavailable());
    }
    let bytes = client
        .download(&entry.key, entry.size_bytes)
        .await
        .map_err(|e| crate::error::upstream(e))?;
    if bytes.len() != entry.size_bytes || digest(&bytes) != entry.sha256 {
        return Err(unavailable());
    }
    let receipt = SavedFile {
        filename: entry.filename.clone(),
        content_type: entry.content_type.clone(),
        bucket: client.bucket().into(),
        storage_key: entry.key.clone(),
        size_bytes: entry.size_bytes,
        sha256: entry.sha256.clone(),
    };
    Ok((receipt, bytes))
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn unavailable() -> AppError {
    AppError::new(
        StatusCode::CONFLICT,
        "Saved report S3 bytes or checkpoint are invalid",
    )
}

#[cfg(test)]
#[path = "../tests/cron/publication.rs"]
mod tests;

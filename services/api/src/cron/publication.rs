use crate::{
    client::vercel_blob::{Blob, VercelBlobClient},
    error::AppError,
    models::cron::checkpoint::{ExecutionCheckpoint, FileCheckpoint},
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
    client: &VercelBlobClient,
    entry: &FileCheckpoint,
    original: Option<&[u8]>,
) -> Result<(Blob, Vec<u8>), AppError> {
    let blob = if let Some(blob) = client
        .metadata(&entry.key)
        .await
        .map_err(|e| crate::error::upstream(e))?
    {
        blob
    } else {
        let bytes = original.ok_or_else(|| {
            AppError::new(
                StatusCode::CONFLICT,
                "Generated report file is missing from Blob; regeneration is disabled",
            )
        })?;
        if bytes.len() != entry.size_bytes || digest(bytes) != entry.sha256 {
            return Err(unavailable());
        }
        match client
            .put(&entry.key, &entry.content_type, bytes.to_vec())
            .await
        {
            Ok(blob) => blob,
            Err(error) => {
                // A lost response can hide an accepted PUT. Reconcile by immutable key, never replay PUT.
                client
                    .metadata(&entry.key)
                    .await
                    .map_err(|e| crate::error::upstream(e))?
                    .ok_or_else(|| crate::error::upstream(error))?
            }
        }
    };
    client
        .validate(&blob, &entry.key, &entry.content_type)
        .map_err(|e| crate::error::upstream(e))?;
    if let Some(receipt) = &entry.receipt {
        let mut saved = blob.clone();
        saved.download_url.clone_from(&receipt.download_url);
        client
            .validate(&saved, &entry.key, &entry.content_type)
            .map_err(|e| crate::error::upstream(e))?;
    }
    let url = entry
        .receipt
        .as_ref()
        .map_or(blob.download_url.as_str(), |r| r.download_url.as_str());
    let bytes = client
        .download(url, entry.size_bytes)
        .await
        .map_err(|e| crate::error::upstream(e))?;
    if bytes.len() != entry.size_bytes || digest(&bytes) != entry.sha256 {
        return Err(unavailable());
    }
    Ok((blob, bytes))
}
pub(crate) fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn unavailable() -> AppError {
    AppError::new(
        StatusCode::CONFLICT,
        "Saved report Blob bytes or checkpoint are invalid",
    )
}

#[cfg(test)]
#[path = "../tests/cron/publication.rs"]
mod tests;

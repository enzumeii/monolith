pub(crate) mod encode;

use crate::{error::AppError, models::report_results::SavedFile};
use axum::http::StatusCode;
use chrono::NaiveDateTime;
use domain::dto::ReportFileFormat;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt};
use uuid::Uuid;

pub(crate) const CSV_CONTENT_TYPE: &str = "text/csv; charset=utf-8";
pub(crate) const XLSX_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

#[derive(Debug)]
pub(crate) struct GeneratedFile {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Manifest {
    pub id: Uuid,
    pub generated_at: NaiveDateTime,
    pub file_format: ReportFileFormat,
    pub files: Vec<FileEntry>,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct FileEntry {
    pub filename: String,
    pub content_type: String,
    pub sha256: String,
}

pub(crate) struct PreparedReport {
    pub manifest: Manifest,
    pub files: Vec<GeneratedFile>,
}

pub(crate) async fn load(root: &Path, id: Uuid) -> Result<Option<PreparedReport>, AppError> {
    for directory in [root.join(id.to_string()), pending(root, id)] {
        match fs::read(directory.join("manifest.json")).await {
            Ok(bytes) => {
                let manifest: Manifest = serde_json::from_slice(&bytes)
                    .map_err(|error| unavailable().with_source(error))?;
                if manifest.id != id || manifest.files.is_empty() {
                    return Err(unavailable());
                }
                let mut files = Vec::new();
                for entry in &manifest.files {
                    validate_entry(entry)?;
                    let bytes = fs::read(directory.join(&entry.filename))
                        .await
                        .map_err(|error| unavailable().with_source(error))?;
                    if digest(&bytes) != entry.sha256 {
                        return Err(unavailable());
                    }
                    files.push(GeneratedFile {
                        filename: entry.filename.clone(),
                        content_type: entry.content_type.clone(),
                        bytes,
                    });
                }
                return Ok(Some(PreparedReport { manifest, files }));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if fs::try_exists(&directory).await? {
                    return Err(unavailable().with_source(error));
                }
            }
            Err(error) => return Err(unavailable().with_source(error)),
        }
    }
    Ok(None)
}

pub(crate) async fn prepare(
    root: &Path,
    id: Uuid,
    generated_at: NaiveDateTime,
    file_format: ReportFileFormat,
    files: Vec<GeneratedFile>,
) -> Result<PreparedReport, AppError> {
    if let Some(report) = load(root, id).await? {
        return Ok(report);
    }
    let entries = files
        .iter()
        .map(|file| FileEntry {
            filename: file.filename.clone(),
            content_type: file.content_type.clone(),
            sha256: digest(&file.bytes),
        })
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return Err(unavailable());
    }
    for entry in &entries {
        validate_entry(entry)?;
    }
    let manifest = Manifest {
        id,
        generated_at,
        file_format,
        files: entries,
    };
    let parent = root.join(".pending");
    fs::create_dir_all(&parent).await?;
    let temporary = parent.join(format!(".{id}-{}", Uuid::new_v4()));
    fs::create_dir(&temporary).await?;
    let result = async {
        for file in &files {
            write_synced(&temporary.join(&file.filename), &file.bytes).await?;
        }
        let encoded =
            serde_json::to_vec(&manifest).map_err(|error| unavailable().with_source(error))?;
        write_synced(&temporary.join("manifest.json"), &encoded).await?;
        sync_directory(&temporary).await?;
        if let Err(error) = fs::rename(&temporary, pending(root, id)).await
            && load(root, id).await?.is_none()
        {
            return Err(AppError::from(error));
        }
        sync_directory(&parent).await?;
        load(root, id).await?.ok_or_else(unavailable)
    }
    .await;
    let _ = fs::remove_dir_all(&temporary).await;
    result
}

pub(crate) async fn publish(root: &Path, id: Uuid) -> Result<Vec<SavedFile>, AppError> {
    let report = load(root, id).await?.ok_or_else(unavailable)?;
    let destination = root.join(id.to_string());
    if !fs::try_exists(&destination).await? {
        if let Err(error) = fs::rename(pending(root, id), &destination).await
            && !fs::try_exists(&destination).await?
        {
            return Err(AppError::from(error));
        }
        sync_directory(root).await?;
        sync_directory(&root.join(".pending")).await?;
    }
    let published = load(root, id).await?.ok_or_else(unavailable)?;
    if published.manifest.generated_at != report.manifest.generated_at {
        return Err(unavailable());
    }
    Ok(published
        .manifest
        .files
        .into_iter()
        .map(|file| SavedFile {
            storage_key: format!("{id}/{}", file.filename),
            download_url: format!(
                "/v1/admin/report-results/{id}/download?file={}",
                file.filename
            ),
            filename: file.filename,
            content_type: file.content_type,
        })
        .collect())
}

pub(crate) async fn read_published(
    root: &Path,
    id: Uuid,
    reference: &SavedFile,
) -> Result<GeneratedFile, AppError> {
    if reference.storage_key != format!("{id}/{}", reference.filename)
        || !fs::try_exists(root.join(id.to_string())).await?
    {
        return Err(unavailable());
    }
    let report = load(root, id).await?.ok_or_else(unavailable)?;
    report
        .files
        .into_iter()
        .find(|file| {
            file.filename == reference.filename && file.content_type == reference.content_type
        })
        .ok_or_else(unavailable)
}

async fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    file.write_all(bytes).await?;
    file.sync_all().await?;
    Ok(())
}

async fn sync_directory(path: &Path) -> Result<(), AppError> {
    fs::File::open(path).await?.sync_all().await?;
    Ok(())
}

fn pending(root: &Path, id: Uuid) -> PathBuf {
    root.join(".pending").join(id.to_string())
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn validate_entry(entry: &FileEntry) -> Result<(), AppError> {
    if entry.filename.is_empty()
        || entry.filename.len() > 200
        || entry.filename.starts_with('.')
        || !entry
            .filename
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        || !matches!(
            entry.content_type.as_str(),
            CSV_CONTENT_TYPE | XLSX_CONTENT_TYPE
        )
    {
        return Err(unavailable());
    }
    Ok(())
}
fn unavailable() -> AppError {
    AppError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Saved report file is unavailable",
    )
}

#[cfg(test)]
#[path = "../tests/services/report_storage.rs"]
mod tests;

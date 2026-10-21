use crate::{config::ReportStorageConfig, error::AppError, models::report_results::SavedFile};
use axum::http::StatusCode;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt};
use uuid::Uuid;

pub(crate) const CSV_CONTENT_TYPE: &str = "text/csv; charset=utf-8";
pub(crate) struct GeneratedFile {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Manifest {
    pub id: Uuid,
    pub generated_at: NaiveDateTime,
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
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct MailIntent {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub html: String,
    pub first_attempt_at: NaiveDateTime,
}

pub(crate) async fn load(root: &Path, id: Uuid) -> Result<Option<PreparedReport>, AppError> {
    let directory = pending(root, id);
    let bytes = match fs::read(directory.join("manifest.json")).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if fs::try_exists(&directory).await?
                || fs::try_exists(root.join("public").join(id.to_string())).await?
            {
                return Err(unavailable());
            }
            return Ok(None);
        }
        Err(error) => return Err(unavailable().with_source(error)),
    };
    let manifest: Manifest =
        serde_json::from_slice(&bytes).map_err(|e| unavailable().with_source(e))?;
    if manifest.id != id || manifest.files.is_empty() {
        return Err(unavailable());
    }
    let published = root.join("public").join(id.to_string());
    let files_root = if fs::try_exists(&published).await? {
        published
    } else {
        directory.join("files")
    };
    let mut files = Vec::new();
    for entry in &manifest.files {
        validate_entry(entry)?;
        let bytes = fs::read(files_root.join(&entry.filename))
            .await
            .map_err(|e| unavailable().with_source(e))?;
        if digest(&bytes) != entry.sha256 {
            return Err(unavailable());
        }
        files.push(GeneratedFile {
            filename: entry.filename.clone(),
            content_type: entry.content_type.clone(),
            bytes,
        });
    }
    Ok(Some(PreparedReport { manifest, files }))
}
pub(crate) async fn prepare(
    root: &Path,
    id: Uuid,
    generated_at: NaiveDateTime,
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
        files: entries,
    };
    let parent = root.join(".pending");
    fs::create_dir_all(&parent).await?;
    let temporary = parent.join(format!(".{id}-{}", Uuid::new_v4()));
    fs::create_dir(&temporary).await?;
    let result = async {
        fs::create_dir(temporary.join("files")).await?;
        for file in &files {
            write_synced(&temporary.join("files").join(&file.filename), &file.bytes).await?;
        }
        write_synced(
            &temporary.join("manifest.json"),
            &serde_json::to_vec(&manifest).map_err(anyhow::Error::new)?,
        )
        .await?;
        sync_directory(&temporary.join("files")).await?;
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
    let _ = fs::remove_dir_all(temporary).await;
    result
}
pub(crate) async fn publish(
    storage: &ReportStorageConfig,
    id: Uuid,
) -> Result<Vec<SavedFile>, AppError> {
    let root = storage.directory();
    load(root, id).await?.ok_or_else(unavailable)?;
    let public = root.join("public");
    fs::create_dir_all(&public).await?;
    let destination = public.join(id.to_string());
    if !fs::try_exists(&destination).await? {
        if let Err(error) = fs::rename(pending(root, id).join("files"), &destination).await
            && !fs::try_exists(&destination).await?
        {
            return Err(error.into());
        }
        sync_directory(&public).await?;
        sync_directory(&pending(root, id)).await?;
    }
    let report = load(root, id).await?.ok_or_else(unavailable)?;
    let base = reqwest::Url::parse(&format!(
        "{}/",
        storage.public_base_url().trim_end_matches('/')
    ))
    .map_err(anyhow::Error::new)?;
    report
        .manifest
        .files
        .into_iter()
        .map(|file| {
            let storage_key = format!("{id}/{}", file.filename);
            let download_url = base
                .join(&storage_key)
                .map_err(anyhow::Error::new)?
                .to_string();
            Ok(SavedFile {
                filename: file.filename,
                content_type: file.content_type,
                storage_key,
                download_url,
            })
        })
        .collect()
}
pub(crate) async fn mail_intent(
    root: &Path,
    id: Uuid,
    proposed: &MailIntent,
) -> Result<MailIntent, AppError> {
    let directory = pending(root, id);
    let path = directory.join("email.json");
    if !fs::try_exists(&path).await? {
        let temporary = directory.join(format!(".email-{}", Uuid::new_v4()));
        write_synced(
            &temporary,
            &serde_json::to_vec(proposed).map_err(anyhow::Error::new)?,
        )
        .await?;
        let result = fs::hard_link(&temporary, &path).await;
        let _ = fs::remove_file(temporary).await;
        if let Err(error) = result
            && error.kind() != std::io::ErrorKind::AlreadyExists
        {
            return Err(error.into());
        }
        sync_directory(&directory).await?;
    }
    serde_json::from_slice(&fs::read(path).await?).map_err(|e| unavailable().with_source(e))
}
pub(crate) async fn has_mail_intent(root: &Path, id: Uuid) -> Result<bool, AppError> {
    Ok(fs::try_exists(pending(root, id).join("email.json")).await?)
}
pub(crate) async fn accepted(root: &Path, id: Uuid) -> Result<bool, AppError> {
    match fs::read(pending(root, id).join("accepted")).await {
        Ok(bytes) if bytes == b"accepted" => Ok(true),
        Ok(_) => Err(unavailable()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
pub(crate) async fn accept(root: &Path, id: Uuid) -> Result<(), AppError> {
    if accepted(root, id).await? {
        return Ok(());
    }
    write_synced(&pending(root, id).join("accepted"), b"accepted").await?;
    sync_directory(&pending(root, id)).await
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
    if !crate::validations::report_results::csv_filename(&entry.filename)
        || entry.content_type != CSV_CONTENT_TYPE
    {
        return Err(unavailable());
    }
    Ok(())
}
fn unavailable() -> AppError {
    AppError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Saved report file or checkpoint is unavailable",
    )
}

#[cfg(test)]
#[path = "../tests/services/report_storage.rs"]
mod tests;

use crate::{error::AppError, models::cron::report::SavedFile};
use chrono::{NaiveDateTime, Timelike};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExecutionCheckpoint {
    pub generated_at: NaiveDateTime,
    pub storage_prefix: String,
    pub files: Vec<FileCheckpoint>,
    pub mail: MailState,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileCheckpoint {
    pub filename: String,
    pub key: String,
    pub content_type: String,
    pub size_bytes: usize,
    pub sha256: String,
    pub receipt: Option<SavedFile>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(
    tag = "state",
    content = "intent",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum MailState {
    NotStarted,
    Sending(MailIntent),
    Accepted(MailIntent),
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MailIntent {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub html: String,
    pub first_attempt_at: NaiveDateTime,
}
impl ExecutionCheckpoint {
    pub(crate) fn validate(&self) -> Result<(), AppError> {
        let parts = self.storage_prefix.split('/').collect::<Vec<_>>();
        if !self.generated_at.nanosecond().is_multiple_of(1000)
            || parts.len() != 3
            || !matches!(parts[0], "inventory" | "transaction")
            || chrono::NaiveDate::parse_from_str(parts[1], "%Y%m%d").is_err()
            || uuid::Uuid::parse_str(parts[2]).is_err()
            || self.files.is_empty()
        {
            return Err(super::saved_invalid());
        }
        let expected: &[&str] = if parts[0] == "inventory" {
            &["inventory.csv"]
        } else {
            &["transactions.csv", "items.csv"]
        };
        if self.files.len() != expected.len() {
            return Err(super::saved_invalid());
        }
        for (file, name) in self.files.iter().zip(expected) {
            if file.filename != *name
                || file.key != format!("{}/{}", self.storage_prefix, name)
                || file.content_type != report_generator::CSV_CONTENT_TYPE
                || file.sha256.len() != 64
                || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(super::saved_invalid());
            }
            if let Some(receipt) = &file.receipt {
                if receipt.filename != file.filename
                    || receipt.storage_key != file.key
                    || receipt.content_type != file.content_type
                {
                    return Err(super::saved_invalid());
                }
                crate::validations::cron::report::validate_saved_file(receipt)?;
            }
        }
        if let MailState::Sending(intent) | MailState::Accepted(intent) = &self.mail
            && (self.files.iter().any(|f| f.receipt.is_none())
                || [&intent.from, &intent.to, &intent.subject, &intent.html]
                    .iter()
                    .any(|s| s.trim().is_empty()))
        {
            return Err(super::saved_invalid());
        }
        Ok(())
    }
    pub(crate) fn advances(&self, previous: Option<&Self>) -> bool {
        let Some(previous) = previous else {
            return matches!(self.mail, MailState::NotStarted)
                && self.files.iter().all(|f| f.receipt.is_none());
        };
        if self.generated_at != previous.generated_at
            || self.storage_prefix != previous.storage_prefix
            || self.files.len() != previous.files.len()
        {
            return false;
        }
        for (next, old) in self.files.iter().zip(&previous.files) {
            let mut expected = old.clone();
            expected.receipt.clone_from(&next.receipt);
            if next != &expected
                || old
                    .receipt
                    .as_ref()
                    .is_some_and(|r| next.receipt.as_ref() != Some(r))
            {
                return false;
            }
        }
        match (&previous.mail, &self.mail) {
            (MailState::NotStarted, MailState::NotStarted | MailState::Sending(_)) => true,
            (MailState::Sending(a), MailState::Sending(b) | MailState::Accepted(b))
            | (MailState::Accepted(a), MailState::Accepted(b)) => a == b,
            _ => false,
        }
    }
    pub(crate) fn receipts(&self) -> Vec<SavedFile> {
        self.files
            .iter()
            .filter_map(|file| file.receipt.clone())
            .collect()
    }
}

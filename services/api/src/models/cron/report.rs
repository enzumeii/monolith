use crate::error::AppError;
use chrono::NaiveDateTime;
use chrono_tz::Asia::Bangkok;
use domain::dto::ScheduleReportType;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedFile {
    pub filename: String,
    pub content_type: String,
    pub bucket: String,
    pub size_bytes: usize,
    pub sha256: String,
    pub storage_key: String,
}
pub(crate) struct ReportResult {
    pub id: Uuid,
    pub report_type: String,
    pub attempted_at: NaiveDateTime,
    pub generated_at: Option<NaiveDateTime>,
    pub status: String,
    pub error: Option<String>,
    pub parameters: String,
    pub files: Vec<SavedFile>,
}
#[derive(Clone, Copy)]
pub(crate) struct ResultFilter {
    pub report_type: Option<ScheduleReportType>,
    pub generated_from: Option<NaiveDateTime>,
    pub generated_to: Option<NaiveDateTime>,
    pub page: u32,
    pub per_page: u32,
}

impl SavedFile {
    pub(crate) fn valid_filename(value: &str) -> bool {
        !value.is_empty()
            && value.len() <= 200
            && !value.starts_with('.')
            && std::path::Path::new(value)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    }
    pub(crate) fn validate(&self) -> Result<(), AppError> {
        let invalid = crate::models::cron::saved_invalid;
        if !Self::valid_filename(&self.filename)
            || self.content_type != report_generator::CSV_CONTENT_TYPE
        {
            return Err(invalid());
        }
        let bucket = &self.bucket;
        if !(3..=63).contains(&bucket.len())
            || bucket.contains("..")
            || !bucket.as_bytes()[0].is_ascii_alphanumeric()
            || !bucket.as_bytes()[bucket.len() - 1].is_ascii_alphanumeric()
            || !bucket
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.'))
            || bucket.parse::<std::net::Ipv4Addr>().is_ok()
        {
            return Err(invalid());
        }
        validate_storage_key(&self.storage_key, &self.filename, &self.sha256)
    }

    pub(crate) fn validate_for_report(
        &self,
        kind: ScheduleReportType,
        id: uuid::Uuid,
        generated_at: Option<NaiveDateTime>,
    ) -> Result<(), AppError> {
        self.validate()?;
        let generated_at = generated_at.ok_or_else(crate::models::cron::saved_invalid)?;
        let kind = match kind {
            ScheduleReportType::Inventory => "inventory",
            ScheduleReportType::Transaction => "transaction",
        };
        let parts = self.storage_key.split('/').collect::<Vec<_>>();
        if parts[0] != kind
            || parts[2] != id.to_string()
            || parts[1]
                != generated_at
                    .and_utc()
                    .with_timezone(&chrono_tz::Asia::Bangkok)
                    .format("%Y%m%d")
                    .to_string()
        {
            return Err(crate::models::cron::saved_invalid());
        }
        Ok(())
    }
}

pub(crate) fn validate_storage_key(
    key: &str,
    filename: &str,
    sha256: &str,
) -> Result<(), AppError> {
    let parts = key.split('/').collect::<Vec<_>>();
    if parts.len() != 5
        || !matches!(
            (parts[0], filename),
            ("inventory", "inventory.csv") | ("transaction", "transactions.csv" | "items.csv")
        )
        || parts[1].len() != 8
        || !parts[1].bytes().all(|b| b.is_ascii_digit())
        || chrono::NaiveDate::parse_from_str(parts[1], "%Y%m%d").is_err()
        || uuid::Uuid::parse_str(parts[2]).map_or(true, |id| id.to_string() != parts[2])
        || sha256.len() != 64
        || !sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || parts[3] != sha256
        || parts[4] != filename
    {
        return Err(crate::models::cron::saved_invalid());
    }
    Ok(())
}

pub(crate) fn storage_prefix(
    kind: ScheduleReportType,
    generated_at: NaiveDateTime,
    id: Uuid,
) -> String {
    let day = generated_at
        .and_utc()
        .with_timezone(&Bangkok)
        .format("%Y%m%d");
    let report_type = match kind {
        ScheduleReportType::Inventory => "inventory",
        ScheduleReportType::Transaction => "transaction",
    };
    format!("{report_type}/{day}/{id}")
}

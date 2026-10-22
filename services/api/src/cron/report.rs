use crate::{
    cron::publication::{GeneratedFile, PreparedReport},
    error::AppError,
    models::cron::checkpoint::{ExecutionCheckpoint, FileCheckpoint, MailState},
    services::cron::report::{self, ReportData},
};
use axum::http::StatusCode;
use chrono::SubsecRound;
use domain::dto::ScheduleReportType;
use report_generator::CSV_CONTENT_TYPE;
use uuid::Uuid;

pub(crate) async fn prepare(
    id: Uuid,
    kind: ScheduleReportType,
    data: ReportData,
    max_payload_bytes: usize,
) -> Result<PreparedReport, AppError> {
    let files = encode(data, max_payload_bytes).await?;
    let generated_at = chrono::Utc::now().naive_utc().trunc_subsecs(6);
    let storage_prefix = report::storage_prefix(kind, generated_at, id);
    let checkpoint = ExecutionCheckpoint {
        generated_at,
        storage_prefix: storage_prefix.clone(),
        files: files
            .iter()
            .map(|file| FileCheckpoint {
                filename: file.filename.clone(),
                key: format!("{storage_prefix}/{}", file.filename),
                content_type: file.content_type.clone(),
                size_bytes: file.bytes.len(),
                sha256: super::publication::digest(&file.bytes),
                receipt: None,
            })
            .collect(),
        mail: MailState::NotStarted,
    };
    checkpoint.validate()?;
    Ok(PreparedReport { checkpoint, files })
}

async fn encode(data: ReportData, limit: usize) -> Result<Vec<GeneratedFile>, AppError> {
    use report_generator::{accounting, inventory};
    let mut files = Vec::new();
    match data {
        ReportData::Inventory(rows) => {
            let mut csv = String::new();
            encode_csv(
                &mut csv,
                &inventory::HEADERS,
                rows.iter().map(inventory_cells),
                inventory::NUMERIC_COLUMNS,
                limit,
            )
            .await?;
            files.push(generated_file("inventory.csv", csv));
        }
        ReportData::Transaction(report) => {
            let mut csv = String::new();
            encode_csv(
                &mut csv,
                accounting::TRANSACTION_HEADERS,
                report
                    .transactions
                    .stripe
                    .iter()
                    .chain(&report.transactions.paypal)
                    .map(transaction_cells),
                accounting::TRANSACTION_NUMERIC_COLUMNS,
                limit,
            )
            .await?;
            let remaining = limit - csv.len();
            files.push(generated_file("transactions.csv", csv));
            let mut csv = String::new();
            encode_csv(
                &mut csv,
                accounting::ITEM_HEADERS,
                report.items.iter().map(accounting::item_cells),
                accounting::ITEM_NUMERIC_COLUMNS,
                remaining,
            )
            .await?;
            files.push(generated_file("items.csv", csv));
        }
    }
    Ok(files)
}

fn inventory_cells(row: &domain::dto::ProductReportRow) -> Vec<String> {
    report_generator::inventory::cells(row).into()
}

fn transaction_cells(row: &domain::dto::accounting::AccountingTransaction) -> Vec<String> {
    report_generator::accounting::transaction_cells(row, "")
}

async fn encode_csv(
    csv: &mut String,
    headers: &[&str],
    rows: impl Iterator<Item = Vec<String>> + Send,
    numeric_columns: &[usize],
    limit: usize,
) -> Result<(), AppError> {
    let mut output = BoundedCsv { csv, limit };
    report_generator::csv::write_row(&mut output, headers, &[]).map_err(|_| payload_error())?;
    for (index, row) in rows.enumerate() {
        report_generator::csv::write_row(&mut output, row, numeric_columns)
            .map_err(|_| payload_error())?;
        if index % 32 == 31 {
            tokio::task::yield_now().await;
        }
    }
    Ok(())
}

fn generated_file(filename: &str, csv: String) -> GeneratedFile {
    GeneratedFile {
        filename: filename.into(),
        content_type: CSV_CONTENT_TYPE.into(),
        bytes: csv.into_bytes(),
    }
}

fn payload_error() -> AppError {
    AppError::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        "Report files exceed configured size limit",
    )
}

struct BoundedCsv<'a> {
    csv: &'a mut String,
    limit: usize,
}
impl std::fmt::Write for BoundedCsv<'_> {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        if value.len() > self.limit.saturating_sub(self.csv.len()) {
            return Err(std::fmt::Error);
        }
        self.csv.push_str(value);
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/cron/report.rs"]
mod tests;

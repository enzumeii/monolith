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
    let files = tokio::task::spawn_blocking(move || encode(data))
        .await
        .map_err(anyhow::Error::new)?;
    if files.iter().map(|file| file.bytes.len()).sum::<usize>() > max_payload_bytes {
        return Err(AppError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Report files exceed configured size limit",
        ));
    }
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

fn encode(data: ReportData) -> Vec<GeneratedFile> {
    let pairs = match data {
        ReportData::Inventory(rows) => {
            vec![("inventory.csv", report_generator::inventory::encode(&rows))]
        }
        ReportData::Transaction(report) => {
            let transactions = report_generator::accounting::transaction_table(
                report
                    .transactions
                    .stripe
                    .iter()
                    .chain(&report.transactions.paypal),
                "",
            );
            let items = report_generator::accounting::item_table(&report.items);
            vec![
                (
                    "transactions.csv",
                    report_generator::csv::encode_table(transactions.headers, &transactions.rows),
                ),
                (
                    "items.csv",
                    report_generator::csv::encode_table(items.headers, &items.rows),
                ),
            ]
        }
    };
    pairs
        .into_iter()
        .map(|(filename, csv)| GeneratedFile {
            filename: filename.into(),
            content_type: CSV_CONTENT_TYPE.into(),
            bytes: csv.into_bytes(),
        })
        .collect()
}

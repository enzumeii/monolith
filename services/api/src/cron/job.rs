use crate::{
    cron::{publication, report},
    error::AppError,
    models::cron::{
        Job,
        checkpoint::{ExecutionCheckpoint, MailIntent, MailState},
        report_type,
    },
    repositories,
    services::cron::{email, report as report_service},
    state::AppState,
    validations,
};
use axum::http::StatusCode;
use chrono::{NaiveDateTime, Utc};
use report_generator::CSV_CONTENT_TYPE;
use resend_rs::types::{CreateAttachment, CreateEmailBaseOptions};
use uuid::Uuid;

pub(crate) struct ClaimedJob {
    pub job: Job,
    pub token: Uuid,
    pub recipient: String,
}
#[derive(Debug)]
pub(crate) struct ClaimFailure {
    pub consumed: bool,
    pub error: AppError,
}
impl From<AppError> for ClaimFailure {
    fn from(error: AppError) -> Self {
        Self {
            consumed: false,
            error,
        }
    }
}
impl From<sqlx::Error> for ClaimFailure {
    fn from(error: sqlx::Error) -> Self {
        AppError::from(error).into()
    }
}
pub(crate) async fn claim(state: &AppState) -> Result<Option<ClaimedJob>, ClaimFailure> {
    let now = Utc::now().naive_utc();
    let Some(configuration) =
        repositories::cron_jobs::candidate(&state.pool, now, max_attempts(state)).await?
    else {
        return Ok(None);
    };
    let mut transaction = state.pool.begin().await?;
    let Some(config) =
        repositories::cron_configurations::lock(&mut transaction, configuration).await?
    else {
        return Ok(None);
    };
    let token = Uuid::new_v4();
    let Some(job) = repositories::cron_jobs::claim(
        &mut transaction,
        configuration,
        now,
        token,
        lease_until(state),
        max_attempts(state),
    )
    .await?
    else {
        return Ok(None);
    };
    // Existing audit context wins over later configuration edits.
    let existing = repositories::report_results::context(&mut transaction, job.id).await?;
    let context = if let Some(existing) = existing {
        Ok(existing)
    } else {
        (|| {
            let kind = report_type(&config.report_type)?;
            Ok::<_, AppError>((
                config.report_type.clone(),
                validations::cron::report::validate_resolved(
                    kind,
                    &config.parameter,
                    job.scheduled_for,
                )?,
            ))
        })()
    };
    let (kind, parameters) = match context {
        Ok(context) => context,
        Err(error) => {
            repositories::cron_jobs::fail(
                &mut transaction,
                job.id,
                token,
                "Invalid saved report configuration",
                None,
            )
            .await?;
            transaction.commit().await?;
            return Err(ClaimFailure {
                consumed: true,
                error,
            });
        }
    };
    repositories::report_results::start(&mut transaction, job.id, &kind, &parameters, now).await?;
    transaction.commit().await?;
    Ok(Some(ClaimedJob {
        job,
        token,
        recipient: config.receiver_email,
    }))
}
pub(crate) struct Failure {
    pub error: AppError,
    pub message: &'static str,
}
pub(crate) async fn execute(state: &AppState, claim: &ClaimedJob) -> Result<(), Failure> {
    let mut phase = "Report generation failed";
    let outcome = async {
        let audit = repositories::report_results::find(&state.pool, claim.job.id)
            .await?
            .ok_or_else(lease_lost)?;
        let (mut checkpoint, original_files) = prepare_report(state, claim, &audit).await?;
        phase = "Stored report publication failed";
        let attachments = publish_report(state, claim, &mut checkpoint, &original_files).await?;
        phase = "Email delivery failed or unconfirmed; may have been sent";
        send_report(
            state,
            claim,
            &mut checkpoint,
            attachments,
            (report_type(&audit.report_type)?, &audit.parameters),
        )
        .await
        .map_err(|e| e.context("email delivery unconfirmed; may have been sent"))?;
        let files = checkpoint.receipts();
        let mut transaction = state.pool.begin().await?;
        if !repositories::cron_jobs::finish(&mut transaction, claim.job.id, claim.token).await?
            || !repositories::report_results::complete(&mut transaction, claim.job.id, &files)
                .await?
        {
            return Err(lease_lost());
        }
        transaction.commit().await?;
        Ok(())
    }
    .await;
    outcome.map_err(|error| Failure {
        error,
        message: phase,
    })
}
pub(crate) async fn fail(
    state: &AppState,
    claim: &ClaimedJob,
    error: &str,
) -> Result<(), AppError> {
    let retry = crate::cron::retry_at(state, claim.job.attempts);
    let mut transaction = state.pool.begin().await?;
    if repositories::cron_jobs::fail(&mut transaction, claim.job.id, claim.token, error, retry)
        .await?
    {
        repositories::report_results::fail(&mut transaction, claim.job.id, error).await?;
    }
    transaction.commit().await?;
    Ok(())
}
pub(crate) fn lease_until(state: &AppState) -> NaiveDateTime {
    Utc::now().naive_utc() + chrono::Duration::seconds(seconds(state.settings.scheduler.lease_secs))
}
pub(crate) fn max_attempts(state: &AppState) -> i32 {
    i32::try_from(state.settings.scheduler.max_attempts).unwrap_or(i32::MAX)
}
pub(crate) fn lease_lost() -> AppError {
    AppError::new(
        StatusCode::CONFLICT,
        "Scheduled report lease is no longer owned",
    )
}
pub(crate) fn report_options(
    from: &str,
    to: &str,
    subject: &str,
    html: &str,
    files: impl IntoIterator<Item = (String, Vec<u8>)>,
    id: Uuid,
) -> resend_rs::idempotent::Idempotent<CreateEmailBaseOptions> {
    CreateEmailBaseOptions::new(from, [to], subject)
        .with_html(html)
        .with_attachments(files.into_iter().map(|(filename, bytes)| {
            CreateAttachment::from_content(bytes)
                .with_filename(&filename)
                .with_content_type(CSV_CONTENT_TYPE)
        }))
        .with_idempotency_key(&format!("report-result-{id}"))
}

async fn prepare_report(
    state: &AppState,
    claim: &ClaimedJob,
    audit: &crate::models::cron::report::ReportResult,
) -> Result<(ExecutionCheckpoint, Vec<publication::GeneratedFile>), AppError> {
    let existing =
        repositories::cron_jobs::checkpoint(&state.pool, claim.job.id, claim.token).await?;
    let mut original_files = Vec::new();
    let checkpoint = if let Some(checkpoint) = existing {
        if audit.generated_at != Some(checkpoint.generated_at)
            || checkpoint.storage_prefix
                != report_service::storage_prefix(
                    report_type(&audit.report_type)?,
                    checkpoint.generated_at,
                    audit.id,
                )
            || audit.files.0 != checkpoint.receipts()
        {
            return Err(crate::models::cron::saved_invalid());
        }
        checkpoint
    } else {
        if audit.generated_at.is_some() || !audit.files.0.is_empty() {
            return Err(AppError::new(
                StatusCode::CONFLICT,
                "Generated checkpoint is missing; regeneration is disabled",
            ));
        }
        let kind = report_type(&audit.report_type)?;
        let shopify = crate::state::required_client(&state.shopify, "Shopify client unavailable")?;
        let providers = match kind {
            domain::dto::ScheduleReportType::Inventory => {
                report_service::Providers::Inventory(shopify)
            }
            domain::dto::ScheduleReportType::Transaction => {
                report_service::Providers::Transaction {
                    shopify,
                    stripe: crate::state::required_client(
                        &state.stripe,
                        "Stripe client unavailable",
                    )?,
                    paypal: crate::state::required_client(
                        &state.paypal,
                        "PayPal client unavailable",
                    )?,
                }
            }
        };
        let data = report_service::generate(audit, providers).await?;
        let prepared = report::prepare(
            audit.id,
            kind,
            data,
            state.settings.scheduler.max_payload_bytes,
        )
        .await?;
        persist(state, claim, None, &prepared.checkpoint).await?;
        original_files = prepared.files;
        prepared.checkpoint
    };
    if checkpoint
        .files
        .iter()
        .map(|f| f.size_bytes)
        .try_fold(0usize, usize::checked_add)
        .is_none_or(|size| size > state.settings.scheduler.max_payload_bytes)
    {
        return Err(AppError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Saved report exceeds configured size limit",
        ));
    }
    Ok((checkpoint, original_files))
}
async fn publish_report(
    state: &AppState,
    claim: &ClaimedJob,
    checkpoint: &mut ExecutionCheckpoint,
    original_files: &[publication::GeneratedFile],
) -> Result<Vec<(String, Vec<u8>)>, AppError> {
    let blob = crate::state::required_client(&state.vercel_blob, "Vercel Blob client unavailable")?;
    let mut attachments = Vec::new();
    let mut publication_error = None;
    for index in 0..checkpoint.files.len() {
        let entry = &checkpoint.files[index];
        let original = original_files
            .iter()
            .find(|file| file.filename == entry.filename)
            .map(|file| file.bytes.as_slice());
        // Recovery checks every object so even an earlier missing file cannot hide a later durable receipt.
        let (remote, bytes) = match publication::obtain(blob, entry, original).await {
            Ok(value) => value,
            Err(error) => {
                if publication_error.is_none() {
                    publication_error = Some(error);
                }
                continue;
            }
        };
        attachments.push((entry.filename.clone(), bytes));
        if entry.receipt.is_none() {
            let mut next = checkpoint.clone();
            next.files[index].receipt = Some(crate::models::cron::report::SavedFile {
                filename: entry.filename.clone(),
                content_type: entry.content_type.clone(),
                storage_key: entry.key.clone(),
                download_url: remote.download_url,
            });
            persist(state, claim, Some(checkpoint), &next).await?;
            *checkpoint = next;
        }
    }
    if let Some(error) = publication_error {
        return Err(error.context("publication"));
    }
    Ok(attachments)
}
async fn persist(
    state: &AppState,
    claim: &ClaimedJob,
    previous: Option<&ExecutionCheckpoint>,
    next: &ExecutionCheckpoint,
) -> Result<(), AppError> {
    let mut transaction = state.pool.begin().await?;
    if !repositories::cron_jobs::save_checkpoint(
        &mut transaction,
        claim.job.id,
        claim.token,
        previous,
        next,
    )
    .await?
    {
        return Err(lease_lost());
    }
    transaction.commit().await?;
    Ok(())
}
async fn send_report(
    state: &AppState,
    claim: &ClaimedJob,
    checkpoint: &mut ExecutionCheckpoint,
    files: Vec<(String, Vec<u8>)>,
    saved_parameters: (domain::dto::ScheduleReportType, &str),
) -> Result<(), AppError> {
    if matches!(checkpoint.mail, MailState::Accepted(_)) {
        return Ok(());
    }
    let size = files
        .iter()
        .map(|(_, bytes)| bytes.len().div_ceil(3).saturating_mul(4))
        .sum::<usize>();
    if size > state.settings.scheduler.max_email_bytes {
        return Err(AppError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Report attachments exceed configured email size limit",
        ));
    }
    let client = crate::state::required_client(&state.resend, "Resend client unavailable")?;
    if matches!(checkpoint.mail, MailState::NotStarted) {
        validations::cron::report::validate_saved_parameters(
            saved_parameters.0,
            saved_parameters.1,
        )?;
        let report = match saved_parameters.0 {
            domain::dto::ScheduleReportType::Inventory => email::Report::Inventory {
                id: saved_parameters.1,
            },
            domain::dto::ScheduleReportType::Transaction => {
                let (start, end) = saved_parameters
                    .1
                    .split_once(" - ")
                    .ok_or_else(crate::models::cron::saved_invalid)?;
                email::Report::Transaction { start, end }
            }
        };
        let content = email::report_content(&claim.recipient, report, checkpoint.generated_at);
        let mut next = checkpoint.clone();
        next.mail = MailState::Sending(MailIntent {
            from: content.from,
            to: content.to,
            subject: content.subject,
            html: content.html,
            first_attempt_at: Utc::now().naive_utc(),
        });
        persist(state, claim, Some(checkpoint), &next).await?;
        *checkpoint = next;
    }
    let MailState::Sending(intent) = &checkpoint.mail else {
        return Err(crate::models::cron::saved_invalid());
    };
    if intent.first_attempt_at <= Utc::now().naive_utc() - chrono::Duration::hours(24) {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "Email delivery unconfirmed; may have been sent; retry window expired",
        ));
    }
    let options = report_options(
        &intent.from,
        &intent.to,
        &intent.subject,
        &intent.html,
        files,
        claim.job.id,
    );
    client.send_email(options).await.map_err(AppError::from)?;
    let mut next = checkpoint.clone();
    next.mail = MailState::Accepted(intent.clone());
    persist(state, claim, Some(checkpoint), &next).await?;
    *checkpoint = next;
    Ok(())
}
fn seconds(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
#[path = "../tests/cron/job.rs"]
mod tests;

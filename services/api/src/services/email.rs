use crate::{
    error::AppError,
    services::report_storage::{self, MailIntent, PreparedReport},
    state::AppState,
};
use axum::http::StatusCode;
use resend_rs::types::{CreateAttachment, CreateEmailBaseOptions};
use uuid::Uuid;

pub(crate) async fn report(
    state: &AppState,
    id: Uuid,
    recipient: &str,
    prepared: PreparedReport,
    recovering: bool,
) -> Result<(), AppError> {
    let root = state.settings.reports.storage.directory();
    if report_storage::accepted(root, id).await? {
        return Ok(());
    }
    let size = prepared
        .files
        .iter()
        .map(|file| file.bytes.len().div_ceil(3).saturating_mul(4))
        .sum::<usize>();
    if size > state.settings.scheduler.max_email_bytes {
        return Err(AppError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Report attachments exceed configured email size limit",
        ));
    }
    let sender = state.settings.shipping_sender.as_ref().ok_or_else(|| {
        AppError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Report sender is not configured",
        )
    })?;
    let client = super::common::resend_client(state)?;
    if recovering && !report_storage::has_mail_intent(root, id).await? {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "Email checkpoint missing; delivery unconfirmed; automatic resend disabled",
        ));
    }
    let intent = report_storage::mail_intent(
        root,
        id,
        &MailIntent {
            from: sender.email.clone(),
            to: recipient.into(),
            subject: "Your scheduled report".into(),
            html: "<p>Your scheduled report is attached.</p>".into(),
            first_attempt_at: chrono::Utc::now().naive_utc(),
        },
    )
    .await?;
    if intent.first_attempt_at <= chrono::Utc::now().naive_utc() - chrono::Duration::hours(24) {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            "Email delivery unconfirmed; may have been sent; retry window expired",
        ));
    }
    let options = CreateEmailBaseOptions::new(&intent.from, [intent.to.as_str()], &intent.subject)
        .with_html(&intent.html)
        .with_attachments(prepared.files.into_iter().map(|file| {
            CreateAttachment::from_content(file.bytes)
                .with_filename(&file.filename)
                .with_content_type(report_storage::CSV_CONTENT_TYPE)
        }))
        .with_idempotency_key(&format!("report-result-{id}"));
    client.send_email(options).await.map_err(AppError::from)?;
    report_storage::accept(root, id).await?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/services/email.rs"]
mod tests;

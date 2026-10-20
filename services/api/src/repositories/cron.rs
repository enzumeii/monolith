use crate::{
    error::AppError,
    models::cron::{CronConfiguration, ReportResult},
};
use chrono::NaiveDateTime;
use domain::dto::{
    EmailStatus, GenerationStatus, SchedulePeriodMode, ScheduleReportType, ScheduleStatus,
};
use serde_json::Value;
use sqlx::{FromRow, PgConnection, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ConfigurationFilter {
    pub report_type: Option<ScheduleReportType>,
    pub status: Option<ScheduleStatus>,
    pub page: u32,
    pub per_page: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResultFilter {
    pub configuration_id: Option<Uuid>,
    pub report_type: Option<ScheduleReportType>,
    pub generation_status: Option<GenerationStatus>,
    pub email_status: Option<EmailStatus>,
    pub scheduled_from: Option<NaiveDateTime>,
    pub scheduled_to: Option<NaiveDateTime>,
    pub page: u32,
    pub per_page: u32,
}

#[derive(FromRow)]
pub(crate) struct ReportPayload {
    pub report_type: String,
    pub generation_status: String,
    pub report_schema_version: i32,
    pub report_data: Option<Value>,
}

#[derive(FromRow)]
pub(crate) struct GenerationJob {
    pub id: Uuid,
    pub report_type: String,
    pub period_start: Option<NaiveDateTime>,
    pub period_end_exclusive: Option<NaiveDateTime>,
    pub inventory_id: Option<String>,
    pub generation_attempts: i32,
}

#[derive(FromRow)]
pub(crate) struct EmailJob {
    pub id: Uuid,
    pub cron_name: String,
    pub receiver_email: String,
    pub report_type: String,
    pub report_data: Value,
    pub email_attempts: i32,
}

#[tracing::instrument(skip(pool))]
pub(crate) async fn list_configurations(
    pool: &PgPool,
    filter: ConfigurationFilter,
) -> Result<Vec<CronConfiguration>, AppError> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT id, name, report_type, cron_expression, receiver_email, period_mode, inventory_id, status, next_run_at, created_at, updated_at FROM cron_configurations WHERE TRUE",
    );
    if let Some(report_type) = filter.report_type {
        query
            .push(" AND report_type = ")
            .push_bind(report_type_value(report_type));
    }
    if let Some(status) = filter.status {
        query.push(" AND status = ").push_bind(status_value(status));
    }
    let limit = i64::from(filter.per_page) + 1;
    let offset = i64::from(filter.page - 1) * i64::from(filter.per_page);
    query
        .push(" ORDER BY created_at DESC, id DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    Ok(query.build_query_as().fetch_all(pool).await?)
}

#[tracing::instrument(skip(pool))]
pub(crate) async fn find_configuration(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<CronConfiguration>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, name, report_type, cron_expression, receiver_email, period_mode, inventory_id, status, next_run_at, created_at, updated_at FROM cron_configurations WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

#[tracing::instrument(skip(connection, name, cron_expression, receiver_email))]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn insert_configuration(
    connection: &mut PgConnection,
    name: &str,
    report_type: ScheduleReportType,
    cron_expression: &str,
    receiver_email: &str,
    period_mode: Option<SchedulePeriodMode>,
    inventory_id: Option<&str>,
    status: ScheduleStatus,
    next_run_at: Option<chrono::NaiveDateTime>,
) -> Result<CronConfiguration, AppError> {
    Ok(sqlx::query_as(
        "INSERT INTO cron_configurations (name, report_type, cron_expression, receiver_email, period_mode, inventory_id, status, next_run_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id, name, report_type, cron_expression, receiver_email, period_mode, inventory_id, status, next_run_at, created_at, updated_at",
    )
    .bind(name)
    .bind(report_type_value(report_type))
    .bind(cron_expression)
    .bind(receiver_email)
    .bind(period_mode.map(period_mode_value))
    .bind(inventory_id)
    .bind(status_value(status))
    .bind(next_run_at)
    .fetch_one(connection)
    .await?)
}

#[tracing::instrument(skip(connection))]
pub(crate) async fn lock_configuration(
    connection: &mut PgConnection,
    id: Uuid,
) -> Result<Option<CronConfiguration>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, name, report_type, cron_expression, receiver_email, period_mode, inventory_id, status, next_run_at, created_at, updated_at FROM cron_configurations WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(connection)
    .await?)
}

#[tracing::instrument(skip(connection, name, cron_expression, receiver_email))]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn update_configuration(
    connection: &mut PgConnection,
    id: Uuid,
    name: &str,
    report_type: ScheduleReportType,
    cron_expression: &str,
    receiver_email: &str,
    period_mode: Option<SchedulePeriodMode>,
    inventory_id: Option<&str>,
    status: ScheduleStatus,
    next_run_at: Option<chrono::NaiveDateTime>,
) -> Result<CronConfiguration, AppError> {
    Ok(sqlx::query_as(
        "UPDATE cron_configurations SET name=$2, report_type=$3, cron_expression=$4, receiver_email=$5, period_mode=$6, inventory_id=$7, status=$8, next_run_at=$9, updated_at=CURRENT_TIMESTAMP AT TIME ZONE 'UTC' WHERE id=$1 RETURNING id, name, report_type, cron_expression, receiver_email, period_mode, inventory_id, status, next_run_at, created_at, updated_at",
    )
    .bind(id)
    .bind(name)
    .bind(report_type_value(report_type))
    .bind(cron_expression)
    .bind(receiver_email)
    .bind(period_mode.map(period_mode_value))
    .bind(inventory_id)
    .bind(status_value(status))
    .bind(next_run_at)
    .fetch_one(connection)
    .await?)
}

#[tracing::instrument(skip(connection))]
pub(crate) async fn delete_configuration(
    connection: &mut PgConnection,
    id: Uuid,
) -> Result<bool, AppError> {
    Ok(sqlx::query("DELETE FROM cron_configurations WHERE id=$1")
        .bind(id)
        .execute(connection)
        .await?
        .rows_affected()
        == 1)
}

#[tracing::instrument(skip(pool))]
pub(crate) async fn list_results(
    pool: &PgPool,
    filter: ResultFilter,
) -> Result<Vec<ReportResult>, AppError> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT id, configuration_id_snapshot, cron_name, cron_expression, report_type, receiver_email, period_mode, inventory_id, scheduled_for, period_start, period_end_exclusive, generation_status, email_status, report_schema_version, row_count, started_at, generated_at, email_first_attempt_at, sent_at, generation_error, email_error, email_message_id FROM report_results WHERE TRUE",
    );
    if let Some(configuration_id) = filter.configuration_id {
        query
            .push(" AND configuration_id_snapshot = ")
            .push_bind(configuration_id);
    }
    if let Some(report_type) = filter.report_type {
        query
            .push(" AND report_type = ")
            .push_bind(report_type_value(report_type));
    }
    if let Some(generation_status) = filter.generation_status {
        query
            .push(" AND generation_status = ")
            .push_bind(generation_status_value(generation_status));
    }
    if let Some(email_status) = filter.email_status {
        query
            .push(" AND email_status = ")
            .push_bind(email_status_value(email_status));
    }
    if let Some(scheduled_from) = filter.scheduled_from {
        query
            .push(" AND scheduled_for >= ")
            .push_bind(scheduled_from);
    }
    if let Some(scheduled_to) = filter.scheduled_to {
        query.push(" AND scheduled_for < ").push_bind(scheduled_to);
    }
    let limit = i64::from(filter.per_page) + 1;
    let offset = i64::from(filter.page - 1) * i64::from(filter.per_page);
    query
        .push(" ORDER BY scheduled_for DESC, id DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    Ok(query.build_query_as().fetch_all(pool).await?)
}

#[tracing::instrument(skip(pool))]
pub(crate) async fn find_result(pool: &PgPool, id: Uuid) -> Result<Option<ReportResult>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, configuration_id_snapshot, cron_name, cron_expression, report_type, receiver_email, period_mode, inventory_id, scheduled_for, period_start, period_end_exclusive, generation_status, email_status, report_schema_version, row_count, started_at, generated_at, email_first_attempt_at, sent_at, generation_error, email_error, email_message_id FROM report_results WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

#[tracing::instrument(skip(pool))]
pub(crate) async fn find_payload(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<ReportPayload>, AppError> {
    Ok(sqlx::query_as(
        "SELECT report_type, generation_status, report_schema_version, report_data FROM report_results WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

#[tracing::instrument(skip(connection))]
pub(crate) async fn lock_due_configurations(
    connection: &mut PgConnection,
    now: NaiveDateTime,
    limit: i64,
) -> Result<Vec<CronConfiguration>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, name, report_type, cron_expression, receiver_email, period_mode, inventory_id, status, next_run_at, created_at, updated_at FROM cron_configurations WHERE status='ENABLED' AND next_run_at <= $1 ORDER BY next_run_at, id LIMIT $2 FOR UPDATE SKIP LOCKED",
    )
    .bind(now)
    .bind(limit)
    .fetch_all(connection)
    .await?)
}

#[tracing::instrument(skip(connection, config))]
pub(crate) async fn insert_queued_result(
    connection: &mut PgConnection,
    config: &CronConfiguration,
    scheduled_for: NaiveDateTime,
    period_start: Option<NaiveDateTime>,
    period_end_exclusive: Option<NaiveDateTime>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO report_results (cron_configuration_id, configuration_id_snapshot, cron_name, cron_expression, report_type, receiver_email, period_mode, inventory_id, scheduled_for, period_start, period_end_exclusive, generation_status, email_status) VALUES ($1,$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'QUEUED','NOT_READY') ON CONFLICT (configuration_id_snapshot, scheduled_for) DO NOTHING",
    )
    .bind(config.id)
    .bind(&config.name)
    .bind(&config.cron_expression)
    .bind(&config.report_type)
    .bind(&config.receiver_email)
    .bind(&config.period_mode)
    .bind(&config.inventory_id)
    .bind(scheduled_for)
    .bind(period_start)
    .bind(period_end_exclusive)
    .execute(connection)
    .await?;
    Ok(())
}

#[tracing::instrument(skip(connection))]
pub(crate) async fn advance_schedule(
    connection: &mut PgConnection,
    id: Uuid,
    next_run_at: NaiveDateTime,
) -> Result<(), AppError> {
    sqlx::query("UPDATE cron_configurations SET next_run_at=$2, updated_at=CURRENT_TIMESTAMP AT TIME ZONE 'UTC' WHERE id=$1 AND status='ENABLED'")
        .bind(id)
        .bind(next_run_at)
        .execute(connection)
        .await?;
    Ok(())
}

pub(crate) async fn claim_generation(
    pool: &PgPool,
    token: Uuid,
    now: NaiveDateTime,
    lease_until: NaiveDateTime,
    max_attempts: i32,
) -> Result<Option<GenerationJob>, AppError> {
    Ok(sqlx::query_as(
        "WITH candidate AS (SELECT id FROM report_results WHERE ((generation_status='QUEUED' AND (next_attempt_at IS NULL OR next_attempt_at <= $1)) OR (generation_status='FAILED' AND generation_attempts < $4 AND (next_attempt_at IS NULL OR next_attempt_at <= $1)) OR (generation_status='RUNNING' AND lease_until <= $1 AND generation_attempts < $4)) ORDER BY scheduled_for, id LIMIT 1 FOR UPDATE SKIP LOCKED) UPDATE report_results AS result SET generation_status='RUNNING', lease_token=$2, lease_until=$3, started_at=COALESCE(result.started_at,$1), generation_attempts=result.generation_attempts+1, next_attempt_at=NULL, generation_error=NULL FROM candidate WHERE result.id=candidate.id RETURNING result.id, result.report_type, result.period_start, result.period_end_exclusive, result.inventory_id, result.generation_attempts",
    )
    .bind(now)
    .bind(token)
    .bind(lease_until)
    .bind(max_attempts)
    .fetch_optional(pool)
    .await?)
}

pub(crate) async fn complete_generation(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    data: &Value,
    row_count: i64,
    generated_at: NaiveDateTime,
) -> Result<bool, AppError> {
    Ok(sqlx::query("UPDATE report_results SET generation_status='SUCCEEDED', email_status='PENDING', report_data=$3, row_count=$4, generated_at=$5, lease_token=NULL, lease_until=NULL, next_attempt_at=NULL WHERE id=$1 AND generation_status='RUNNING' AND lease_token=$2")
        .bind(id).bind(token).bind(data).bind(row_count).bind(generated_at).execute(pool).await?.rows_affected() == 1)
}

pub(crate) async fn fail_generation(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    message: &str,
    retry_at: Option<NaiveDateTime>,
) -> Result<bool, AppError> {
    Ok(sqlx::query("UPDATE report_results SET generation_status='FAILED', email_status='NOT_READY', generation_error=$3, next_attempt_at=$4, lease_token=NULL, lease_until=NULL WHERE id=$1 AND generation_status='RUNNING' AND lease_token=$2")
        .bind(id).bind(token).bind(message).bind(retry_at).execute(pool).await?.rows_affected() == 1)
}

pub(crate) async fn mark_ambiguous_emails_unknown(
    pool: &PgPool,
    cutoff: NaiveDateTime,
    now: NaiveDateTime,
) -> Result<(), AppError> {
    sqlx::query("UPDATE report_results SET email_status='UNKNOWN', lease_token=NULL, lease_until=NULL, next_attempt_at=NULL, email_error='Email outcome could not be confirmed within 24 hours' WHERE email_status='SENDING' AND lease_until <= $1 AND email_first_attempt_at <= $2")
        .bind(now).bind(cutoff).execute(pool).await?;
    Ok(())
}

pub(crate) async fn claim_email(
    pool: &PgPool,
    token: Uuid,
    now: NaiveDateTime,
    lease_until: NaiveDateTime,
    max_attempts: i32,
) -> Result<Option<EmailJob>, AppError> {
    Ok(sqlx::query_as("WITH candidate AS (SELECT id FROM report_results WHERE ((email_status='PENDING' AND (next_attempt_at IS NULL OR next_attempt_at <= $1)) OR (email_status='FAILED' AND email_attempts < $4 AND (next_attempt_at IS NULL OR next_attempt_at <= $1)) OR (email_status='SENDING' AND lease_until <= $1 AND email_first_attempt_at > $1 - INTERVAL '24 hours')) ORDER BY scheduled_for, id LIMIT 1 FOR UPDATE SKIP LOCKED) UPDATE report_results AS result SET email_status='SENDING', lease_token=$2, lease_until=$3, email_attempts=result.email_attempts+1, email_first_attempt_at=COALESCE(result.email_first_attempt_at,$1), next_attempt_at=NULL, email_error=NULL FROM candidate WHERE result.id=candidate.id RETURNING result.id, result.cron_name, result.receiver_email, result.report_type, result.report_data, result.email_attempts")
        .bind(now).bind(token).bind(lease_until).bind(max_attempts).fetch_optional(pool).await?)
}

pub(crate) async fn complete_email(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    message_id: &str,
    now: NaiveDateTime,
) -> Result<bool, AppError> {
    Ok(sqlx::query("UPDATE report_results SET email_status='SENT', email_message_id=$3, sent_at=$4, lease_token=NULL, lease_until=NULL, next_attempt_at=NULL WHERE id=$1 AND email_status='SENDING' AND lease_token=$2")
        .bind(id).bind(token).bind(message_id).bind(now).execute(pool).await?.rows_affected()==1)
}

pub(crate) async fn fail_email(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    message: &str,
    retry_at: Option<NaiveDateTime>,
    ambiguous: bool,
) -> Result<bool, AppError> {
    let status = if ambiguous { "SENDING" } else { "FAILED" };
    Ok(sqlx::query("UPDATE report_results SET email_status=$3, email_error=$4, next_attempt_at=$5, lease_until=$5, lease_token=NULL WHERE id=$1 AND email_status='SENDING' AND lease_token=$2")
        .bind(id).bind(token).bind(status).bind(message).bind(retry_at).execute(pool).await?.rows_affected()==1)
}

fn report_type_value(value: ScheduleReportType) -> &'static str {
    match value {
        ScheduleReportType::Transaction => "TRANSACTION",
        ScheduleReportType::Inventory => "INVENTORY",
    }
}

fn period_mode_value(value: SchedulePeriodMode) -> &'static str {
    match value {
        SchedulePeriodMode::PreviousMonth => "PREVIOUS_MONTH",
        SchedulePeriodMode::FirstHalfMonth => "FIRST_HALF_MONTH",
    }
}

fn status_value(value: ScheduleStatus) -> &'static str {
    match value {
        ScheduleStatus::Enabled => "ENABLED",
        ScheduleStatus::Disabled => "DISABLED",
    }
}

fn generation_status_value(value: GenerationStatus) -> &'static str {
    match value {
        GenerationStatus::Queued => "QUEUED",
        GenerationStatus::Running => "RUNNING",
        GenerationStatus::Succeeded => "SUCCEEDED",
        GenerationStatus::Failed => "FAILED",
    }
}

fn email_status_value(value: EmailStatus) -> &'static str {
    match value {
        EmailStatus::NotReady => "NOT_READY",
        EmailStatus::Pending => "PENDING",
        EmailStatus::Sending => "SENDING",
        EmailStatus::Sent => "SENT",
        EmailStatus::Failed => "FAILED",
        EmailStatus::Unknown => "UNKNOWN",
    }
}

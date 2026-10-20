use crate::{
    error::AppError,
    models::cron::{ConfigurationFilter, CronConfiguration, Job},
};
use chrono::NaiveDateTime;
use domain::dto::{ReportFileFormat, SchedulePeriodMode, ScheduleReportType, ScheduleStatus};
use sqlx::{PgConnection, PgPool, Postgres, QueryBuilder};
use uuid::Uuid;

#[tracing::instrument(skip(pool))]
pub(crate) async fn list_configurations(
    pool: &PgPool,
    filter: ConfigurationFilter,
) -> Result<Vec<CronConfiguration>, AppError> {
    let mut query = QueryBuilder::<Postgres>::new(
        r"
        SELECT
            id, name, report_type, cron_expression,
            receiver_email, file_format, period_mode, inventory_id,
            status, next_run_at, created_at, updated_at
        FROM cron_configurations
        WHERE TRUE
    ",
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
        r"
        SELECT
            id, name, report_type, cron_expression,
            receiver_email, file_format, period_mode, inventory_id,
            status, next_run_at, created_at, updated_at
        FROM cron_configurations
        WHERE id=$1
    ",
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
    file_format: ReportFileFormat,
) -> Result<CronConfiguration, AppError> {
    Ok(sqlx::query_as(
        r"
        INSERT INTO cron_configurations (
            name, report_type, cron_expression, receiver_email,
            file_format, period_mode, inventory_id, status,
            next_run_at
        )
        VALUES ($1, $2, $3, $4, $9, $5, $6, $7, $8)
        RETURNING
            id, name, report_type, cron_expression,
            receiver_email, file_format, period_mode, inventory_id,
            status, next_run_at, created_at, updated_at
    ",
    )
    .bind(name)
    .bind(report_type_value(report_type))
    .bind(cron_expression)
    .bind(receiver_email)
    .bind(period_mode.map(period_mode_value))
    .bind(inventory_id)
    .bind(status_value(status))
    .bind(next_run_at)
    .bind(format_value(file_format))
    .fetch_one(connection)
    .await?)
}

#[tracing::instrument(skip(connection))]
pub(crate) async fn lock_configuration(
    connection: &mut PgConnection,
    id: Uuid,
) -> Result<Option<CronConfiguration>, AppError> {
    Ok(sqlx::query_as(
        r"
        SELECT
            id, name, report_type, cron_expression,
            receiver_email, file_format, period_mode, inventory_id,
            status, next_run_at, created_at, updated_at
        FROM cron_configurations
        WHERE id=$1 FOR UPDATE
    ",
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
    file_format: ReportFileFormat,
) -> Result<CronConfiguration, AppError> {
    Ok(sqlx::query_as(
        r"
        UPDATE cron_configurations
        SET name=$2, report_type=$3, cron_expression=$4, receiver_email=$5, file_format=$10, period_mode=$6, inventory_id=$7, status=$8, next_run_at=$9, updated_at=CURRENT_TIMESTAMP AT TIME ZONE 'UTC'
        WHERE id=$1
        RETURNING
            id, name, report_type, cron_expression,
            receiver_email, file_format, period_mode, inventory_id,
            status, next_run_at, created_at, updated_at
    ",
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
    .bind(format_value(file_format))
    .fetch_one(connection)
    .await?)
}

#[tracing::instrument(skip(connection))]
pub(crate) async fn delete_configuration(
    connection: &mut PgConnection,
    id: Uuid,
) -> Result<bool, AppError> {
    Ok(sqlx::query(
        r"
        DELETE
        FROM cron_configurations
        WHERE id=$1
    ",
    )
    .bind(id)
    .execute(connection)
    .await?
    .rows_affected()
        == 1)
}

#[tracing::instrument(skip(connection))]
pub(crate) async fn lock_due_configurations(
    connection: &mut PgConnection,
    now: NaiveDateTime,
    limit: i64,
) -> Result<Vec<CronConfiguration>, AppError> {
    Ok(sqlx::query_as(
        r"
        SELECT
            id, name, report_type, cron_expression,
            receiver_email, file_format, period_mode, inventory_id,
            status, next_run_at, created_at, updated_at
        FROM cron_configurations
        WHERE status='ENABLED' AND next_run_at <= $1
        ORDER BY next_run_at, id
        LIMIT $2
        FOR UPDATE SKIP LOCKED
    ",
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
        r"
        INSERT INTO cron_jobs (
            cron_configuration_id, configuration_id_snapshot, cron_name, cron_expression,
            report_type, receiver_email, file_format, period_mode,
            inventory_id, scheduled_for, period_start, period_end_exclusive,
            generation_status, email_status
        )
        VALUES ($1,$1,$2,$3,$4,$5,$11,$6,$7,$8,$9,$10,'QUEUED','NOT_READY')
        ON CONFLICT (configuration_id_snapshot, scheduled_for) DO NOTHING
    ",
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
    .bind(&config.file_format)
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
    sqlx::query(
        r"
        UPDATE cron_configurations
        SET next_run_at=$2, updated_at=CURRENT_TIMESTAMP AT TIME ZONE 'UTC'
        WHERE id=$1 AND status='ENABLED'
    ",
    )
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
) -> Result<Option<Job>, AppError> {
    Ok(sqlx::query_as(r"
        WITH candidate AS (
            SELECT id FROM cron_jobs
            WHERE generation_attempts < $4 AND (
                ((generation_status = 'QUEUED' OR (generation_status = 'FAILED' AND next_attempt_at IS NOT NULL)) AND (next_attempt_at IS NULL OR next_attempt_at <= $1))
                OR (generation_status = 'RUNNING' AND lease_until <= $1)
            )
            ORDER BY scheduled_for, id
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        ),
        changed AS (
            UPDATE cron_jobs AS job
            SET generation_status = 'RUNNING',
            lease_token = $2, lease_until = $3,
            started_at = COALESCE(job.started_at, $1),
            generation_attempts = job.generation_attempts + 1,
            next_attempt_at = NULL, generation_error = NULL
            FROM candidate
            WHERE job.id = candidate.id
            RETURNING job.*
        ), audit AS (
            INSERT INTO report_results (id, report_type, attempted_at, generated_at, generation_status, email_status,
                generation_error, email_error, period_start, period_end_exclusive, inventory_id)
            SELECT id, report_type, started_at, generated_at, generation_status, email_status,
            generation_error, email_error, period_start, period_end_exclusive, inventory_id
            FROM changed
            ON CONFLICT (id) DO UPDATE
            SET generated_at = EXCLUDED.generated_at, generation_status = EXCLUDED.generation_status,
            email_status = EXCLUDED.email_status, generation_error = EXCLUDED.generation_error,
            email_error = EXCLUDED.email_error
            RETURNING id
        )
        SELECT changed.* FROM changed JOIN audit USING (id)
    ")
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
    generated_at: NaiveDateTime,
) -> Result<bool, AppError> {
    Ok(sqlx::query(r"
        WITH changed AS (
            UPDATE cron_jobs
            SET generation_status = 'SUCCEEDED', email_status = 'PENDING', files_ready = TRUE,
            generated_at = $3, lease_token = NULL, lease_until = NULL, next_attempt_at = NULL
            WHERE id = $1 AND generation_status = 'RUNNING' AND lease_token = $2
            AND lease_until > (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
            RETURNING *
        )
        INSERT INTO report_results (id, report_type, attempted_at, generated_at, generation_status, email_status,
            generation_error, email_error, period_start, period_end_exclusive, inventory_id)
        SELECT id, report_type, started_at, generated_at, generation_status, email_status,
        generation_error, email_error, period_start, period_end_exclusive, inventory_id
        FROM changed
        ON CONFLICT (id) DO UPDATE
        SET generated_at = EXCLUDED.generated_at, generation_status = EXCLUDED.generation_status,
        email_status = EXCLUDED.email_status, generation_error = EXCLUDED.generation_error,
        email_error = EXCLUDED.email_error
        RETURNING id
    ")
    .bind(id)
    .bind(token)
    .bind(generated_at)
    .execute(pool)
    .await?.rows_affected() == 1)
}

pub(crate) async fn fail_generation(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    message: &str,
    retry_at: Option<NaiveDateTime>,
) -> Result<bool, AppError> {
    Ok(sqlx::query(r"
        WITH changed AS (
            UPDATE cron_jobs
            SET generation_status = 'FAILED', email_status = 'NOT_READY', generation_error = $3,
            next_attempt_at = $4, lease_token = NULL, lease_until = NULL
            WHERE id = $1 AND generation_status = 'RUNNING' AND lease_token = $2
            AND lease_until > (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
            RETURNING *
        )
        INSERT INTO report_results (id, report_type, attempted_at, generated_at, generation_status, email_status,
            generation_error, email_error, period_start, period_end_exclusive, inventory_id)
        SELECT id, report_type, started_at, generated_at, generation_status, email_status,
        generation_error, email_error, period_start, period_end_exclusive, inventory_id
        FROM changed
        ON CONFLICT (id) DO UPDATE
        SET generated_at = EXCLUDED.generated_at, generation_status = EXCLUDED.generation_status,
        email_status = EXCLUDED.email_status, generation_error = EXCLUDED.generation_error,
        email_error = EXCLUDED.email_error
        RETURNING id
    ")
    .bind(id)
    .bind(token)
    .bind(message)
    .bind(retry_at)
    .execute(pool)
    .await?.rows_affected() == 1)
}

pub(crate) async fn recover_expired(
    pool: &PgPool,
    now: NaiveDateTime,
    max_attempts: i32,
) -> Result<(), AppError> {
    sqlx::query(r"
        WITH changed AS (
            UPDATE cron_jobs
            SET generation_status = 'FAILED', generation_error = 'Generation lease expired on final attempt',
            lease_token = NULL, lease_until = NULL, next_attempt_at = NULL
            WHERE generation_status = 'RUNNING' AND lease_until <= $1 AND generation_attempts >= $2
            RETURNING *
        )
        INSERT INTO report_results (id, report_type, attempted_at, generated_at, generation_status, email_status,
            generation_error, email_error, period_start, period_end_exclusive, inventory_id)
        SELECT id, report_type, started_at, generated_at, generation_status, email_status,
        generation_error, email_error, period_start, period_end_exclusive, inventory_id
        FROM changed
        ON CONFLICT (id) DO UPDATE
        SET generation_status = EXCLUDED.generation_status, email_status = EXCLUDED.email_status,
        generation_error = EXCLUDED.generation_error, email_error = EXCLUDED.email_error
    ")
    .bind(now)
    .bind(max_attempts)
    .execute(pool)
    .await?;
    sqlx::query(r"
        WITH changed AS (
            UPDATE cron_jobs
            SET email_status = 'UNKNOWN', lease_token = NULL, lease_until = NULL, next_attempt_at = NULL,
            email_error = 'Email outcome could not be confirmed within retry limits'
            WHERE email_status = 'SENDING' AND lease_until <= $1
            AND (email_first_attempt_at <= $1 - INTERVAL '24 hours' OR email_attempts >= $2)
            RETURNING *
        )
        INSERT INTO report_results (id, report_type, attempted_at, generated_at, generation_status, email_status,
            generation_error, email_error, period_start, period_end_exclusive, inventory_id)
        SELECT id, report_type, started_at, generated_at, generation_status, email_status,
        generation_error, email_error, period_start, period_end_exclusive, inventory_id
        FROM changed
        ON CONFLICT (id) DO UPDATE
        SET generation_status = EXCLUDED.generation_status, email_status = EXCLUDED.email_status,
        generation_error = EXCLUDED.generation_error, email_error = EXCLUDED.email_error
    ")
    .bind(now)
    .bind(max_attempts)
    .execute(pool)
    .await?;
    sqlx::query(
        r"
        UPDATE cron_jobs
        SET publication_error = 'Publication lease expired on final attempt',
            lease_token = NULL,
            lease_until = NULL,
            next_attempt_at = NULL
        WHERE email_status = 'SENT'
            AND published_at IS NULL
            AND lease_until <= $1
            AND publication_attempts >= $2
    ",
    )
    .bind(now)
    .bind(max_attempts)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn claim_email(
    pool: &PgPool,
    token: Uuid,
    now: NaiveDateTime,
    lease_until: NaiveDateTime,
    max_attempts: i32,
) -> Result<Option<Job>, AppError> {
    Ok(sqlx::query_as(r"
        WITH candidate AS (
            SELECT id FROM cron_jobs
            WHERE files_ready AND email_attempts < $4
            AND (email_first_attempt_at IS NULL OR email_first_attempt_at > $1 - INTERVAL '24 hours')
            AND (
                ((email_status = 'PENDING' OR (email_status = 'FAILED' AND next_attempt_at IS NOT NULL)) AND (next_attempt_at IS NULL OR next_attempt_at <= $1))
                OR (email_status = 'SENDING' AND lease_until <= $1)
            )
            ORDER BY scheduled_for, id
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        ),
        changed AS (
            UPDATE cron_jobs AS job
            SET email_status = 'SENDING', lease_token = $2, lease_until = $3,
            email_attempts = job.email_attempts + 1,
            email_first_attempt_at = COALESCE(job.email_first_attempt_at, $1),
            next_attempt_at = NULL, email_error = NULL
            FROM candidate
            WHERE job.id = candidate.id
            RETURNING job.*
        ), audit AS (
            INSERT INTO report_results (id, report_type, attempted_at, generated_at, generation_status, email_status,
                generation_error, email_error, period_start, period_end_exclusive, inventory_id)
            SELECT id, report_type, started_at, generated_at, generation_status, email_status,
            generation_error, email_error, period_start, period_end_exclusive, inventory_id
            FROM changed
            ON CONFLICT (id) DO UPDATE
            SET generated_at = EXCLUDED.generated_at, generation_status = EXCLUDED.generation_status,
            email_status = EXCLUDED.email_status, generation_error = EXCLUDED.generation_error,
            email_error = EXCLUDED.email_error
            RETURNING id
        )
        SELECT changed.* FROM changed JOIN audit USING (id)
    ")
    .bind(now)
    .bind(token)
    .bind(lease_until)
    .bind(max_attempts)
    .fetch_optional(pool)
    .await?)
}

pub(crate) async fn complete_email(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    message_id: &str,
    now: NaiveDateTime,
) -> Result<bool, AppError> {
    Ok(sqlx::query(r"
        WITH changed AS (
            UPDATE cron_jobs
            SET email_status = 'SENT', email_message_id = $3, sent_at = $4,
            lease_token = NULL, lease_until = NULL, next_attempt_at = NULL
            WHERE id = $1 AND email_status = 'SENDING' AND lease_token = $2
            AND lease_until > (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
            RETURNING *
        )
        INSERT INTO report_results (id, report_type, attempted_at, generated_at, generation_status, email_status,
            generation_error, email_error, period_start, period_end_exclusive, inventory_id)
        SELECT id, report_type, started_at, generated_at, generation_status, email_status,
        generation_error, email_error, period_start, period_end_exclusive, inventory_id
        FROM changed
        ON CONFLICT (id) DO UPDATE
        SET generated_at = EXCLUDED.generated_at, generation_status = EXCLUDED.generation_status,
        email_status = EXCLUDED.email_status, generation_error = EXCLUDED.generation_error,
        email_error = EXCLUDED.email_error
        RETURNING id
    ")
    .bind(id)
    .bind(token)
    .bind(message_id)
    .bind(now)
    .execute(pool)
    .await?.rows_affected() == 1)
}

pub(crate) async fn fail_email(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    message: &str,
    retry_at: Option<NaiveDateTime>,
    ambiguous: bool,
) -> Result<bool, AppError> {
    Ok(sqlx::query(r"
        WITH changed AS (
            UPDATE cron_jobs
            SET email_status = $3, email_error = $4, next_attempt_at = $5,
            lease_until = COALESCE($5, (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')), lease_token = NULL
            WHERE id = $1 AND email_status = 'SENDING' AND lease_token = $2
            AND lease_until > (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
            RETURNING *
        )
        INSERT INTO report_results (id, report_type, attempted_at, generated_at, generation_status, email_status,
            generation_error, email_error, period_start, period_end_exclusive, inventory_id)
        SELECT id, report_type, started_at, generated_at, generation_status, email_status,
        generation_error, email_error, period_start, period_end_exclusive, inventory_id
        FROM changed
        ON CONFLICT (id) DO UPDATE
        SET generated_at = EXCLUDED.generated_at, generation_status = EXCLUDED.generation_status,
        email_status = EXCLUDED.email_status, generation_error = EXCLUDED.generation_error,
        email_error = EXCLUDED.email_error
        RETURNING id
    ")
    .bind(id)
    .bind(token)
    .bind(if ambiguous { "SENDING" } else { "FAILED" })
        .bind(message)
    .bind(retry_at)
    .execute(pool)
    .await?.rows_affected() == 1)
}

pub(crate) async fn renew_lease(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    until: NaiveDateTime,
) -> Result<bool, AppError> {
    Ok(sqlx::query(
        r"
        UPDATE cron_jobs
        SET lease_until = $3
        WHERE id = $1 AND lease_token = $2 AND lease_until > (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
    ",
    )
    .bind(id)
    .bind(token)
    .bind(until)
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
}

pub(crate) async fn claim_publication(
    pool: &PgPool,
    token: Uuid,
    now: NaiveDateTime,
    until: NaiveDateTime,
    max_attempts: i32,
) -> Result<Option<Job>, AppError> {
    Ok(sqlx::query_as(
        r"
        WITH candidate AS (
            SELECT id FROM cron_jobs
            WHERE email_status = 'SENT' AND published_at IS NULL
            AND publication_attempts < $4
            AND (next_attempt_at IS NULL OR next_attempt_at <= $1)
            AND (lease_until IS NULL OR lease_until <= $1)
            ORDER BY sent_at, id
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE cron_jobs AS job
        SET lease_token = $2, lease_until = $3, publication_attempts = job.publication_attempts + 1,
        next_attempt_at = NULL, publication_error = NULL
        FROM candidate
        WHERE job.id = candidate.id
        RETURNING job.*
    ",
    )
    .bind(now)
    .bind(token)
    .bind(until)
    .bind(max_attempts)
    .fetch_optional(pool)
    .await?)
}

pub(crate) async fn finish_publication(
    connection: &mut PgConnection,
    id: Uuid,
    token: Option<Uuid>,
) -> Result<bool, AppError> {
    Ok(sqlx::query(r"
        UPDATE cron_jobs
        SET published_at = (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'), files_ready = TRUE,
        publication_error = NULL,
        lease_token = NULL, lease_until = NULL, next_attempt_at = NULL
        WHERE id = $1
        AND ($2::uuid IS NULL OR (lease_token = $2 AND lease_until > (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')))
    ")
    .bind(id)
    .bind(token)
    .execute(connection)
    .await?.rows_affected() == 1)
}

pub(crate) async fn record_publication_failure(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    retry_at: Option<NaiveDateTime>,
) -> Result<(), AppError> {
    sqlx::query(
        r"
        UPDATE cron_jobs
        SET publication_error = 'Stored report publication failed',
        next_attempt_at = $3, lease_token = NULL, lease_until = NULL
        WHERE id = $1 AND published_at IS NULL AND lease_token = $2
        AND lease_until > (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
    ",
    )
    .bind(id)
    .bind(token)
    .bind(retry_at)
    .execute(pool)
    .await?;
    Ok(())
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

fn format_value(value: ReportFileFormat) -> &'static str {
    match value {
        ReportFileFormat::Csv => "CSV",
        ReportFileFormat::Xlsx => "XLSX",
    }
}

#[cfg(test)]
#[path = "../tests/repositories/cron.rs"]
pub(crate) mod tests;

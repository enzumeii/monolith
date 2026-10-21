use crate::{error::AppError, models::cron::Job};
use chrono::NaiveDateTime;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub(crate) async fn latest(
    connection: &mut PgConnection,
    cron_config_id: Uuid,
) -> Result<Option<NaiveDateTime>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT MAX(scheduled_for) FROM cron_jobs WHERE cron_configuration_id = $1",
    )
    .bind(cron_config_id)
    .fetch_one(connection)
    .await?)
}
pub(crate) async fn enqueue(
    connection: &mut PgConnection,
    cron_config_id: Uuid,
    due: NaiveDateTime,
) -> Result<(), AppError> {
    sqlx::query(
        r"
        INSERT INTO cron_jobs (cron_configuration_id, scheduled_for)
        VALUES ($1, $2) ON CONFLICT (cron_configuration_id, scheduled_for) DO NOTHING
    ",
    )
    .bind(cron_config_id)
    .bind(due)
    .execute(connection)
    .await?;
    Ok(())
}
pub(crate) async fn candidate(
    pool: &PgPool,
    now: NaiveDateTime,
    max: i32,
) -> Result<Option<Uuid>, AppError> {
    Ok(sqlx::query_scalar(
        r"
        SELECT cron_configuration_id FROM cron_jobs
        WHERE attempts < $2 AND (
            (status = 'PENDING' AND (lease_until IS NULL OR lease_until <= $1))
            OR (status = 'FAILED' AND lease_until <= $1)
        )
        ORDER BY scheduled_for, id LIMIT 1
    ",
    )
    .bind(now)
    .bind(max)
    .fetch_optional(pool)
    .await?)
}
pub(crate) async fn claim(
    connection: &mut PgConnection,
    cron_config_id: Uuid,
    now: NaiveDateTime,
    token: Uuid,
    until: NaiveDateTime,
    max: i32,
) -> Result<Option<Job>, AppError> {
    Ok(sqlx::query_as(
        r"
        WITH candidate AS (
            SELECT id FROM cron_jobs
            WHERE cron_configuration_id = $1 AND attempts < $5 AND (
                (status = 'PENDING' AND (lease_until IS NULL OR lease_until <= $2))
                OR (status = 'FAILED' AND lease_until <= $2)
            )
            ORDER BY scheduled_for, id LIMIT 1 FOR UPDATE SKIP LOCKED
        )
        UPDATE cron_jobs AS job
        SET status = 'PENDING', attempts = attempts + 1, lease_token = $3, lease_until = $4
        FROM candidate WHERE job.id = candidate.id
        RETURNING job.id, job.cron_configuration_id, job.scheduled_for, job.attempts
    ",
    )
    .bind(cron_config_id)
    .bind(now)
    .bind(token)
    .bind(until)
    .bind(max)
    .fetch_optional(connection)
    .await?)
}
pub(crate) async fn renew(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    until: NaiveDateTime,
) -> Result<bool, AppError> {
    Ok(sqlx::query(
        r"
        UPDATE cron_jobs SET lease_until = $3
        WHERE id = $1 AND lease_token = $2 AND status = 'PENDING'
            AND lease_until > (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
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
pub(crate) async fn finish(
    connection: &mut PgConnection,
    id: Uuid,
    token: Uuid,
) -> Result<bool, AppError> {
    Ok(sqlx::query(r"
        UPDATE cron_jobs SET status = 'SUCCESS', error = NULL,
            published_at = (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'), lease_token = NULL, lease_until = NULL
        WHERE id = $1 AND lease_token = $2 AND status = 'PENDING'
            AND lease_until > (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
    ").bind(id).bind(token).execute(connection).await?.rows_affected() == 1)
}
pub(crate) async fn fail(
    connection: &mut PgConnection,
    id: Uuid,
    token: Uuid,
    error: &str,
    retry: Option<NaiveDateTime>,
) -> Result<bool, AppError> {
    Ok(sqlx::query(
        r"
        UPDATE cron_jobs SET status = 'FAILED', error = $3, lease_token = NULL, lease_until = $4
        WHERE id = $1 AND lease_token = $2 AND status = 'PENDING'
            AND lease_until > (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
    ",
    )
    .bind(id)
    .bind(token)
    .bind(error)
    .bind(retry)
    .execute(connection)
    .await?
    .rows_affected()
        == 1)
}
pub(crate) async fn recover(pool: &PgPool, now: NaiveDateTime, max: i32) -> Result<(), AppError> {
    sqlx::query(
        r"
        WITH expired AS (
            UPDATE cron_jobs SET status = 'FAILED',
                error = CASE WHEN attempts >= $2 THEN 'Workflow lease expired on final attempt'
                    ELSE 'Workflow lease expired; email delivery may be unconfirmed' END,
                lease_token = NULL,
                lease_until = CASE WHEN attempts >= $2 THEN NULL ELSE lease_until END
            WHERE status = 'PENDING' AND lease_token IS NOT NULL AND lease_until <= $1
            RETURNING id, error
        )
        UPDATE report_results AS audit SET status = 'FAILED', error = expired.error
        FROM expired WHERE audit.id = expired.id
    ",
    )
    .bind(now)
    .bind(max)
    .execute(pool)
    .await?;
    Ok(())
}
pub(crate) async fn cancel(
    connection: &mut PgConnection,
    cron_config_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query(r"
        WITH jobs AS (
            SELECT id FROM cron_jobs WHERE cron_configuration_id = $1 AND status <> 'SUCCESS' FOR UPDATE
        )
        UPDATE report_results SET status = 'FAILED',
            error = 'Schedule deleted; unfinished work cancelled; in-flight email delivery may have occurred'
        WHERE id IN (SELECT id FROM jobs)
    ").bind(cron_config_id).execute(connection).await?;
    Ok(())
}

pub(crate) async fn checkpoint(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
) -> Result<Option<crate::models::cron::checkpoint::ExecutionCheckpoint>, AppError> {
    let value: Option<Option<sqlx::types::Json<crate::models::cron::checkpoint::ExecutionCheckpoint>>> = sqlx::query_scalar(
        "SELECT execution_checkpoint FROM cron_jobs WHERE id=$1 AND lease_token=$2 AND status='PENDING' AND lease_until > (clock_timestamp() AT TIME ZONE 'UTC')"
    ).bind(id).bind(token).fetch_optional(pool).await?;
    let checkpoint = value.ok_or_else(crate::cron::job::lease_lost)?.map(|v| v.0);
    if let Some(checkpoint) = &checkpoint {
        checkpoint.validate()?;
    }
    Ok(checkpoint)
}
pub(crate) async fn save_checkpoint(
    connection: &mut PgConnection,
    id: Uuid,
    token: Uuid,
    previous: Option<&crate::models::cron::checkpoint::ExecutionCheckpoint>,
    next: &crate::models::cron::checkpoint::ExecutionCheckpoint,
) -> Result<bool, AppError> {
    next.validate()?;
    if !next.advances(previous) {
        return Err(crate::models::cron::saved_invalid());
    }
    let locked: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM cron_jobs WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *connection)
            .await?;
    if locked.is_none() {
        return Ok(false);
    }
    let updated = sqlx::query("UPDATE cron_jobs SET execution_checkpoint=$4 WHERE id=$1 AND lease_token=$2 AND status='PENDING' AND lease_until > (clock_timestamp() AT TIME ZONE 'UTC') AND execution_checkpoint IS NOT DISTINCT FROM $3")
        .bind(id).bind(token).bind(previous.map(sqlx::types::Json)).bind(sqlx::types::Json(next)).execute(&mut *connection).await?.rows_affected() == 1;
    if !updated {
        return Ok(false);
    }
    Ok(sqlx::query("UPDATE report_results SET generated_at=$2, files=$3 WHERE id=$1 AND (generated_at IS NULL OR generated_at=$2)")
        .bind(id).bind(next.generated_at).bind(sqlx::types::Json(next.receipts())).execute(connection).await?.rows_affected() == 1)
}

#[cfg(test)]
#[path = "../tests/repositories/cron_jobs.rs"]
pub(crate) mod tests;

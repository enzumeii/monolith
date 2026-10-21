use crate::{error::AppError, models::cron::Job};
use chrono::NaiveDateTime;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub(crate) async fn latest(
    connection: &mut PgConnection,
    configuration: Uuid,
) -> Result<Option<NaiveDateTime>, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT MAX(scheduled_for) FROM cron_jobs WHERE cron_configuration_id = $1",
    )
    .bind(configuration)
    .fetch_one(connection)
    .await?)
}
pub(crate) async fn enqueue(
    connection: &mut PgConnection,
    configuration: Uuid,
    due: NaiveDateTime,
) -> Result<(), AppError> {
    sqlx::query(
        r"
        INSERT INTO cron_jobs (cron_configuration_id, scheduled_for)
        VALUES ($1, $2) ON CONFLICT (cron_configuration_id, scheduled_for) DO NOTHING
    ",
    )
    .bind(configuration)
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
    configuration: Uuid,
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
    .bind(configuration)
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
    configuration: Uuid,
) -> Result<(), AppError> {
    sqlx::query(r"
        WITH jobs AS (
            SELECT id FROM cron_jobs WHERE cron_configuration_id = $1 AND status <> 'SUCCESS' FOR UPDATE
        )
        UPDATE report_results SET status = 'FAILED',
            error = 'Schedule deleted; unfinished work cancelled; in-flight email delivery may have occurred'
        WHERE id IN (SELECT id FROM jobs)
    ").bind(configuration).execute(connection).await?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/repositories/cron_jobs.rs"]
pub(crate) mod tests;

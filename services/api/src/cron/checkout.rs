use crate::{
    config::CheckoutRecoveryConfig,
    error::AppError,
    models::checkout::{Checkout, RecoveryCode},
    repositories::checkout as repository,
    services::checkout::{self as service, CheckoutClients, RecoveryOutcome},
    state::{AppState, required_client},
};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use std::time::Duration;
use tokio::{
    sync::watch,
    task::{JoinHandle, JoinSet},
    time::Instant,
};
use uuid::Uuid;

pub(crate) async fn run(state: &AppState, work_deadline: Instant) -> Result<(), AppError> {
    let (_shutdown, mut stop) = watch::channel(false);
    run_until(state, work_deadline, &mut stop).await
}

pub(super) fn start(state: &AppState) -> (Option<JoinHandle<()>>, Option<watch::Sender<bool>>) {
    if state.settings.checkout.is_none() || !state.settings.checkout_recovery.enabled {
        return (None, None);
    }
    let state = state.clone();
    let (shutdown, mut stop) = watch::channel(false);
    let period = Duration::from_secs(state.settings.checkout_recovery.wake_secs);
    let task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = stop.changed() => break,
                _ = interval.tick() => {}
            }
            let deadline =
                Instant::now() + Duration::from_secs(state.settings.checkout_recovery.budget_secs);
            if run_until(&state, deadline, &mut stop).await.is_err() {
                tracing::warn!(code = ?RecoveryCode::TransientFailure, "Checkout recovery cycle incomplete");
            }
            if *stop.borrow() {
                break;
            }
        }
    });
    (Some(task), Some(shutdown))
}

pub(super) async fn run_until(
    state: &AppState,
    work_deadline: Instant,
    stop: &mut watch::Receiver<bool>,
) -> Result<(), AppError> {
    let settings = &state.settings.checkout_recovery;
    if !settings.enabled || state.settings.checkout.is_none() || *stop.borrow() {
        return Ok(());
    }
    let deadline = work_deadline.min(Instant::now() + Duration::from_secs(settings.budget_secs));
    let mut tasks = JoinSet::new();
    let mut dispatched = 0;
    let mut exhausted = false;
    let mut failure = None;
    loop {
        while !exhausted
            && dispatched < settings.batch_size
            && tasks.len() < settings.concurrency
            && Instant::now() < deadline
            && !*stop.borrow()
        {
            dispatched += 1;
            let state = state.clone();
            tasks.spawn(async move {
                tokio::time::timeout_at(deadline, execute_one(&state))
                    .await
                    .unwrap_or_else(|_| Err(capacity_error()))
            });
        }
        if tasks.is_empty() {
            break;
        }
        tokio::select! {
            _ = stop.changed() => {
                tasks.abort_all();
                while tasks.join_next().await.is_some() {}
                return Ok(());
            }
            outcome = tasks.join_next() => {
                match outcome {
                    Some(Ok(Ok(claimed))) => exhausted |= !claimed,
                    Some(Ok(Err(error))) => {
                        if failure.is_none() {
                            failure = Some(error);
                        }
                    }
                    Some(Err(_)) => {
                        if failure.is_none() {
                            failure = Some(unavailable());
                        }
                    }
                    None => break,
                }
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

struct Disposition {
    failures: u32,
    next_due: Option<DateTime<Utc>>,
    manual: bool,
    code: RecoveryCode,
}

async fn execute_one(state: &AppState) -> Result<bool, AppError> {
    let settings = &state.settings.checkout_recovery;
    let token = Uuid::new_v4();
    let Some(claim) = repository::claim_due(
        &state.pool,
        token,
        settings.lease_secs,
        settings.max_failures,
    )
    .await
    .map_err(|_| unavailable())?
    else {
        return Ok(false);
    };
    match claim {
        repository::DueClaim::Stopped { id } => {
            tracing::warn!(checkout_id = %id, code = ?RecoveryCode::AttemptsExhausted, "Checkout recovery needs operator review");
        }
        repository::DueClaim::Abandoned { id, failures } => {
            let disposition = retry(settings, failures, RecoveryCode::ClaimAbandoned);
            finish(state, id, token, disposition).await?;
        }
        repository::DueClaim::Ready {
            mut checkout,
            failures,
        } => {
            let id = checkout.id;
            let work = async {
                let outcome = match clients(state, &checkout) {
                    Ok(clients) => {
                        let Some(checkout_settings) = state.settings.checkout.as_ref() else {
                            return Err(unavailable());
                        };
                        service::recover_claimed(
                            &state.pool,
                            checkout_settings,
                            &clients,
                            &mut checkout,
                            token,
                        )
                        .await
                        .map_err(|_| unavailable())?
                    }
                    Err(_) => RecoveryOutcome::Retry(RecoveryCode::TransientFailure),
                };
                let disposition = disposition(settings, failures, outcome);
                tracing::info!(checkout_id = %id, provider = %checkout.provider, payment_state = ?checkout.payment_state, completion_state = ?checkout.completion_state, code = ?disposition.code, "Checkout recovery observed");
                finish(state, id, token, disposition).await
            };
            with_lease(state, id, token, work).await?;
        }
    }
    Ok(true)
}

fn clients<'a>(state: &'a AppState, checkout: &Checkout) -> Result<CheckoutClients<'a>, AppError> {
    Ok(CheckoutClients {
        shopify: required_client(&state.shopify, "Shopify client is not available")?,
        stripe: if checkout.provider == "stripe" {
            Some(required_client(
                &state.stripe,
                "Stripe client is not available",
            )?)
        } else {
            None
        },
        paypal: if checkout.provider == "paypal" {
            Some(required_client(
                &state.paypal,
                "PayPal client is not available",
            )?)
        } else {
            None
        },
    })
}

async fn with_lease<T>(
    state: &AppState,
    id: Uuid,
    token: Uuid,
    work: impl std::future::Future<Output = Result<T, AppError>>,
) -> Result<T, AppError> {
    let lease_secs = state.settings.checkout_recovery.lease_secs;
    let period = Duration::from_millis((u64::from(lease_secs) * 1000 / 3).max(1));
    let mut heartbeat = tokio::time::interval_at(Instant::now() + period, period);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let renewal = async {
        loop {
            heartbeat.tick().await;
            repository::renew(&state.pool, id, token, lease_secs)
                .await
                .map_err(|_| unavailable())?;
        }
    };
    tokio::select! { result = work => result, result = renewal => result }
}

async fn finish(
    state: &AppState,
    id: Uuid,
    token: Uuid,
    disposition: Disposition,
) -> Result<(), AppError> {
    repository::finish_recovery(
        &state.pool,
        id,
        token,
        disposition.failures,
        disposition.next_due,
        disposition.manual,
        disposition.code,
    )
    .await
    .map_err(|_| unavailable())
}

fn disposition(
    settings: &CheckoutRecoveryConfig,
    failures: u32,
    outcome: RecoveryOutcome,
) -> Disposition {
    match outcome {
        RecoveryOutcome::Observed(code) => Disposition {
            failures: 0,
            next_due: Some(
                Utc::now()
                    + chrono::Duration::seconds(
                        i64::try_from(settings.wake_secs).unwrap_or(i64::MAX),
                    ),
            ),
            manual: false,
            code,
        },
        RecoveryOutcome::Retry(code) => retry(settings, failures.saturating_add(1), code),
        RecoveryOutcome::Manual(code) => Disposition {
            failures,
            next_due: None,
            manual: true,
            code,
        },
    }
}

fn retry(settings: &CheckoutRecoveryConfig, failures: u32, code: RecoveryCode) -> Disposition {
    let exhausted = failures >= settings.max_failures;
    Disposition {
        failures,
        next_due: (!exhausted).then(|| {
            Utc::now()
                + chrono::Duration::seconds(
                    i64::try_from(backoff(settings, failures)).unwrap_or(i64::MAX),
                )
        }),
        manual: exhausted,
        code: if exhausted {
            RecoveryCode::AttemptsExhausted
        } else {
            code
        },
    }
}

fn backoff(settings: &CheckoutRecoveryConfig, failures: u32) -> u64 {
    settings
        .retry_backoff_secs
        .saturating_mul(2_u64.saturating_pow(failures.saturating_sub(1)))
        .min(settings.max_retry_backoff_secs)
}

fn unavailable() -> AppError {
    AppError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "Checkout recovery could not retain ownership; unfinished work will recover on a later trigger",
    )
}

fn capacity_error() -> AppError {
    AppError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "Checkout recovery exceeded invocation capacity; unfinished work will recover on a later trigger",
    )
}

#[cfg(test)]
#[path = "../tests/cron/checkout.rs"]
mod tests;

#![allow(clippy::unwrap_used)]
use super::*;

fn settings() -> CheckoutRecoveryConfig {
    CheckoutRecoveryConfig {
        enabled: true,
        wake_secs: 60,
        batch_size: 10,
        concurrency: 2,
        budget_secs: 60,
        lease_secs: 90,
        retry_backoff_secs: 60,
        max_retry_backoff_secs: 900,
        max_failures: 12,
    }
}

#[test]
fn exponential_backoff_caps_without_overflow() {
    let settings = settings();
    let delays: Vec<_> = (1..=7)
        .map(|failures| backoff(&settings, failures))
        .collect();
    assert_eq!(delays, [60, 120, 240, 480, 900, 900, 900]);
    assert_eq!(backoff(&settings, u32::MAX), 900);
}

#[test]
fn successful_waiting_resets_failures_and_schedules_observation() {
    let now = Utc::now();
    let result = disposition(
        &settings(),
        11,
        RecoveryOutcome::Observed(RecoveryCode::WaitingForPayment),
    );
    assert_eq!(result.failures, 0);
    assert!(!result.manual);
    assert_eq!(result.code, RecoveryCode::WaitingForPayment);
    let next = result.next_due.unwrap();
    assert!(next >= now + chrono::Duration::seconds(60));
    assert!(next < now + chrono::Duration::seconds(61));
}

#[test]
fn ordinary_failure_increments_once_and_abandonment_uses_existing_count() {
    let settings = settings();
    let now = Utc::now();
    let ordinary = disposition(
        &settings,
        2,
        RecoveryOutcome::Retry(RecoveryCode::TransientFailure),
    );
    let abandoned = retry(&settings, 3, RecoveryCode::ClaimAbandoned);
    assert_eq!(ordinary.failures, 3);
    assert_eq!(abandoned.failures, 3);
    assert!(!ordinary.manual);
    assert!(!abandoned.manual);
    for next in [ordinary.next_due.unwrap(), abandoned.next_due.unwrap()] {
        assert!(next >= now + chrono::Duration::seconds(240));
        assert!(next < now + chrono::Duration::seconds(241));
    }
}

#[test]
fn exhaustion_and_manual_outcomes_stop_automatic_work() {
    let settings = settings();
    for count in [12, 50] {
        let result = retry(&settings, count, RecoveryCode::TransientFailure);
        assert_eq!(result.failures, count);
        assert!(result.manual);
        assert_eq!(result.next_due, None);
        assert_eq!(result.code, RecoveryCode::AttemptsExhausted);
    }
    let result = disposition(
        &settings,
        3,
        RecoveryOutcome::Manual(RecoveryCode::EvidenceConflict),
    );
    assert_eq!(result.failures, 3);
    assert!(result.manual);
    assert_eq!(result.next_due, None);
    assert_eq!(result.code, RecoveryCode::EvidenceConflict);
}

fn state() -> AppState {
    let settings = config::Config::builder()
        .add_source(config::File::with_name("application"))
        .add_source(config::File::with_name("application.local"))
        .set_override("browser.allowed_origins", vec!["http://localhost:8084"])
        .unwrap()
        .set_override(
            "database.url",
            "postgres://fixture:fixture@localhost:1/fixture",
        )
        .unwrap()
        .set_override("jwt.secret", "ab".repeat(32))
        .unwrap()
        .set_override("scheduler.enabled", false)
        .unwrap()
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@localhost:1/fixture")
        .unwrap();
    crate::state::app_state(pool, settings).unwrap()
}

#[tokio::test]
async fn unconfigured_and_disabled_recovery_never_claims() {
    let mut state = state();
    std::sync::Arc::make_mut(&mut state.settings).checkout = None;
    let deadline = Instant::now() + Duration::from_secs(1);
    tokio::time::timeout_at(deadline, run(&state, deadline))
        .await
        .unwrap()
        .unwrap();
    let (task, shutdown) = start(&state);
    assert!(task.is_none());
    assert!(shutdown.is_none());
    let settings = std::sync::Arc::make_mut(&mut state.settings);
    settings.checkout_recovery.enabled = false;
    settings.checkout = Some(checkout_settings());
    tokio::time::timeout_at(deadline, run(&state, deadline))
        .await
        .unwrap()
        .unwrap();
    let (task, shutdown) = start(&state);
    assert!(task.is_none());
    assert!(shutdown.is_none());
    state.pool.close().await;
}

#[tokio::test]
async fn report_disabled_checkout_task_joins_when_shutdown_precedes_first_tick() {
    let mut state = state();
    std::sync::Arc::make_mut(&mut state.settings).checkout = Some(checkout_settings());
    assert!(!state.settings.scheduler.enabled);
    let (task, shutdown) = start(&state);
    shutdown.unwrap().send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(1), task.unwrap())
        .await
        .unwrap()
        .unwrap();
    state.pool.close().await;
}

fn checkout_settings() -> crate::config::CheckoutSettings {
    serde_json::from_value(serde_json::json!({
        "frontend_origin":"http://localhost:8084",
        "return_path_template":"/{convention_slug}/checkout/return?checkout_id={checkout_id}",
        "cancel_path_template":"/{convention_slug}/checkout/cancel?checkout_id={checkout_id}",
        "shopify_location_id":"1",
        "reservation_secs":3600,
        "guest_recovery_secs":604_800,
        "stripe_livemode":false
    }))
    .unwrap()
}

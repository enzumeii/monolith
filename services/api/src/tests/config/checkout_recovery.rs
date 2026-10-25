#![allow(clippy::unwrap_used)]
use super::*;
use config::{Config, File};

fn settings() -> CheckoutRecoveryConfig {
    Config::builder()
        .add_source(File::with_name("application"))
        .build()
        .unwrap()
        .get("checkout_recovery")
        .unwrap()
}

#[test]
fn explicit_defaults_and_file_overrides_are_loaded() {
    let settings = settings();
    assert!(settings.enabled);
    assert_eq!(
        (
            settings.wake_secs,
            settings.batch_size,
            settings.concurrency
        ),
        (60, 10, 2)
    );
    assert_eq!((settings.budget_secs, settings.lease_secs), (60, 90));
    assert_eq!(
        (
            settings.retry_backoff_secs,
            settings.max_retry_backoff_secs,
            settings.max_failures
        ),
        (60, 900, 12)
    );
    settings.validate().unwrap();
    let overrides: CheckoutRecoveryConfig = Config::builder()
        .add_source(File::with_name("application"))
        .set_override("checkout_recovery.enabled", false)
        .unwrap()
        .set_override("checkout_recovery.wake_secs", 1)
        .unwrap()
        .set_override("checkout_recovery.lease_secs", 1)
        .unwrap()
        .build()
        .unwrap()
        .get("checkout_recovery")
        .unwrap();
    assert!(!overrides.enabled);
    assert_eq!((overrides.wake_secs, overrides.lease_secs), (1, 1));
    overrides.validate().unwrap();
}

#[test]
fn invalid_intervals_limits_and_backoff_order_are_rejected() {
    for field in [
        "wake_secs",
        "batch_size",
        "concurrency",
        "budget_secs",
        "lease_secs",
        "retry_backoff_secs",
        "max_retry_backoff_secs",
        "max_failures",
    ] {
        for value in [0, 100_000] {
            let invalid: CheckoutRecoveryConfig = Config::builder()
                .add_source(File::with_name("application"))
                .set_override(format!("checkout_recovery.{field}"), value)
                .unwrap()
                .build()
                .unwrap()
                .get("checkout_recovery")
                .unwrap();
            assert!(invalid.validate().is_err(), "{field}={value}");
        }
    }
    let mut invalid = settings();
    invalid.concurrency = invalid.batch_size + 1;
    assert!(invalid.validate().is_err());
    invalid.concurrency = 1;
    invalid.max_retry_backoff_secs = invalid.retry_backoff_secs - 1;
    assert!(invalid.validate().is_err());
}

#[test]
fn recovery_budget_is_independent_of_shorter_vercel_invocation_budget() {
    let settings: crate::config::Settings = Config::builder()
        .add_source(File::with_name("application"))
        .add_source(File::with_name("application.local"))
        .set_override("browser.allowed_origins", vec!["http://localhost:8084"])
        .unwrap()
        .set_override("database.url", "postgres://unused")
        .unwrap()
        .set_override("jwt.secret", "ab".repeat(32))
        .unwrap()
        .set_override("scheduler.invocation_budget_secs", 6)
        .unwrap()
        .set_override("scheduler.retention_reserve_secs", 5)
        .unwrap()
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();
    settings.validate().unwrap();
    assert_eq!(settings.checkout_recovery.budget_secs, 60);
}

#[test]
fn settings_require_explicit_recovery_configuration() {
    assert!(serde_json::from_value::<CheckoutRecoveryConfig>(serde_json::json!({})).is_err());
}

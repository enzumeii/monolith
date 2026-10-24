use super::*;

#[test]
fn filters_round_trip_and_reset_page_when_filter_changes() {
    let filters = Filters::parse("report_type=INVENTORY&enabled=false&page=3&per_page=50");
    assert_eq!(filters.page, 3);
    assert_eq!(filters.enabled, Some(false));
    assert_eq!(Filters::parse(&filters.query()), filters);
    assert_eq!(
        Filters::parse("page=0&enabled=garbage&per_page=999"),
        Filters::default()
    );
    let changed = filters.with_type("TRANSACTION");
    assert_eq!(changed.page, 1);
    assert_eq!(changed.enabled, Some(false));
    assert_eq!(changed.per_page, 50);
}

#[test]
fn conflict_requires_successful_reload_even_after_edits_or_failed_retries() {
    let mut recovery = SaveRecovery::default();
    recovery.save_failed(crate::api::ApiError::new(
        reqwest::StatusCode::CONFLICT,
        "Changed; reload",
    ));
    assert!(recovery.reload_required);
    recovery.edited();
    assert!(recovery.failure.is_none());
    assert!(
        recovery.reload_required,
        "editing cannot bypass the conflict recovery"
    );
    recovery.transient_failed(crate::api::ApiError::new(
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        "Reload failed",
    ));
    assert!(
        recovery.reload_required,
        "failed reload does not permit saving"
    );
    recovery.loaded();
    assert!(!recovery.reload_required);
    assert!(recovery.failure.is_none());
}

use super::*;
#[test]
fn local_bounds_allow_one_side_and_reject_invalid_equal_or_reversed_ranges() {
    assert_eq!(
        local_bound("").expect("valid Bangkok datetime fixture"),
        None
    );
    assert_eq!(
        local_bound("2026-10-16T00:00")
            .expect("valid Bangkok datetime fixture")
            .as_deref(),
        Some("2026-10-16T00:00:00+07:00")
    );
    assert_eq!(
        local_bound("2026-10-15T23:59:59.999999999")
            .expect("valid Bangkok datetime fixture")
            .as_deref(),
        Some("2026-10-15T23:59:59.999999999+07:00")
    );
    assert!(local_bound("2026-10-16T00:00Z").is_err());
    assert!(local_bound("2026-02-30T00:00").is_err());
    assert!(validate_bounds(Some("2026-10-01T00:00:00+07:00"), None).is_ok());
    assert!(validate_bounds(None, Some("2026-10-01T00:00:00+07:00")).is_ok());
    assert!(
        validate_bounds(
            Some("2026-10-16T00:00:00+07:00"),
            Some("2026-10-15T17:00:00Z")
        )
        .is_err()
    );
    assert!(
        validate_bounds(
            Some("2026-10-17T00:00:00+07:00"),
            Some("2026-10-16T00:00:00+07:00")
        )
        .is_err()
    );
    assert!(validate_bounds(Some("invalid"), None).is_err());
}
#[test]
fn bangkok_preview_periods_keep_midnight_exclusive_and_fractional_boundaries() {
    for (input, want) in [
        ("2026-10-31T17:00:00Z", "2026-11-01 00:00:00 +07:00"),
        ("2026-09-30T17:00:00Z", "2026-10-01 00:00:00 +07:00"),
        ("2026-10-15T17:00:00Z", "2026-10-16 00:00:00 +07:00"),
        (
            "2026-10-15T16:59:59.999999999Z",
            "2026-10-15 23:59:59.999999999 +07:00",
        ),
    ] {
        assert_eq!(bangkok_time(input), want);
    }
    assert_eq!(local_control("2026-10-31T17:00:00Z"), "2026-11-01T00:00:00");
    assert_eq!(
        bangkok_time("invalid returned time"),
        "invalid returned time"
    );
}

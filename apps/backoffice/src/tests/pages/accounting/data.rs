use super::*;
#[test]
fn calendar_dates_preserve_seconds_and_explicit_bangkok_offset() {
    let request =
        date_range("2024-02-29T00:01:02", "2024-03-01T23:59:59").expect("valid leap date");
    assert_eq!(request.date_from, "2024-02-29T00:01:02+07:00");
    assert_eq!(request.date_to, "2024-03-01T23:59:59+07:00");
    assert_eq!(
        date_range("2026-10-01T00:00", "2026-10-31T23:59")
            .expect("minutes")
            .date_to,
        "2026-10-31T23:59:00+07:00"
    );
}
#[test]
fn invalid_or_nonforward_calendar_ranges_are_rejected() {
    for (start, end) in [
        ("", "2026-10-31T23:59:59"),
        ("2026-10-01T00:00:00", ""),
        ("not-a-date", "2026-10-31T23:59:59"),
        ("2026-02-29T00:00:00", "2026-03-01T00:00:00"),
        ("2026-10-01T25:00:00", "2026-10-31T00:00:00"),
        ("2026-10-01T00:00:00Z", "2026-10-31T00:00:00"),
        ("2026-10-01T00:00:00", "2026-10-01T00:00:00"),
        ("2026-10-02T00:00:00", "2026-10-01T00:00:00"),
    ] {
        assert!(date_range(start, end).is_err(), "{start} -> {end}");
    }
}

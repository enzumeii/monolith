use super::*;

#[test]
fn calendar_respects_leap_years_and_monday_alignment() {
    let february = NaiveDate::from_ymd_opt(2024, 2, 1).expect("valid fixture date");
    let cells = calendar_days(february);
    assert!(cells[..3].iter().all(Option::is_none));
    assert_eq!(cells.iter().flatten().count(), 29);
    assert_eq!(cells.len() % 7, 0);
    assert_eq!(
        calendar_days(february.with_year(2025).expect("valid fixture date"))
            .iter()
            .flatten()
            .count(),
        28
    );
}

#[test]
fn month_navigation_crosses_years_without_clamping_day() {
    let january = NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid fixture date");
    assert_eq!(
        shift_month(january, -1),
        NaiveDate::from_ymd_opt(2025, 12, 1).expect("valid fixture date")
    );
    assert_eq!(shift_month(shift_month(january, -1), 1), january);
    let minimum = NaiveDate::from_ymd_opt(1, 1, 1).expect("valid fixture date");
    assert_eq!(shift_month(minimum, -1), minimum);
    let maximum = NaiveDate::from_ymd_opt(9999, 12, 1).expect("valid fixture date");
    assert_eq!(shift_month(maximum, 1), maximum);
}

#[test]
fn changing_date_preserves_time_seconds_and_fraction_exactly() {
    let target = NaiveDate::from_ymd_opt(2024, 2, 29).expect("valid fixture date");
    assert_eq!(
        replace_date("2026-10-22T18:27:43.123456789", target),
        "2024-02-29T18:27:43.123456789"
    );
    assert_eq!(replace_date("2026-10-22T18:27", target), "2024-02-29T18:27");
    assert_eq!(replace_date("", target), "2024-02-29T00:00");
}

#[test]
fn changing_hour_or_minute_keeps_other_units_and_local_date() {
    let date = NaiveDate::from_ymd_opt(2026, 10, 22).expect("valid fixture date");
    assert_eq!(change_clock("18:27", true, "03"), "03:27");
    assert_eq!(change_clock("18:27", false, "04"), "18:04");
    assert_eq!(
        replace_time("2024-02-29T18:27:43.1200", "03:27", date),
        "2024-02-29T03:27:43.1200"
    );
    assert_eq!(
        replace_time("2024-02-29T18:27", "18:04", date),
        "2024-02-29T18:04"
    );
}

#[test]
fn optional_empty_values_stay_empty_until_explicit_choice() {
    assert_eq!(selected_date(""), None);
    assert_eq!(
        clock_parts(""),
        (String::new(), String::new(), String::new())
    );
    assert_eq!(change_clock("", true, ""), "");
    assert_eq!(change_clock("", true, "09"), "09:00");
    assert_eq!(change_clock("", false, "30"), "00:30");
    let date = NaiveDate::from_ymd_opt(2026, 10, 22).expect("valid fixture date");
    assert_eq!(replace_time("", "", date), "");
    assert_eq!(replace_time("", "09:00", date), "2026-10-22T09:00");
}

#[test]
fn untouched_local_values_round_trip_without_timezone_conversion() {
    for value in [
        "2024-02-29T00:00",
        "2026-10-22T23:59:59",
        "2026-10-22T23:59:59.000001",
    ] {
        let date = selected_date(value).expect("valid fixture date");
        assert_eq!(replace_date(value, date), value);
        assert_eq!(replace_time(value, &value[11..16], date), value);
    }
}

#[test]
fn changing_seconds_replaces_existing_seconds_without_duplicating_precision() {
    let date = NaiveDate::from_ymd_opt(2026, 10, 22).expect("valid fixture date");
    for second in ["00", "59"] {
        let time = format!("18:27:{second}");
        assert_eq!(
            replace_time("2024-02-29T18:27:43.1200", &time, date),
            format!("2024-02-29T18:27:{second}.1200")
        );
        assert_eq!(
            replace_time("2024-02-29T18:27", &time, date),
            format!("2024-02-29T18:27:{second}")
        );
    }
}

#[test]
fn changing_hour_and_minute_preserves_full_clock_precision() {
    assert_eq!(change_clock("18:27:43", true, "03"), "03:27:43");
    assert_eq!(change_clock("18:27:43", false, "04"), "18:04:43");
    assert_eq!(change_clock("18:27:43.1200", false, "04"), "18:04:43.1200");
}

#[test]
fn second_selection_preserves_hour_minute_and_fraction() {
    assert_eq!(
        clock_parts("18:27:43.1200"),
        ("18".into(), "27".into(), "43".into())
    );
    assert_eq!(change_second("18:27:43.1200", "00"), "18:27:00.1200");
    assert_eq!(change_second("18:27:00.1200", "59"), "18:27:59.1200");
    assert_eq!(change_second("18:27", "59"), "18:27:59");
    assert_eq!(change_second("", ""), "");
    assert_eq!(change_second("", "00"), "00:00:00");
    assert_eq!(change_second("", "59"), "00:00:59");
}

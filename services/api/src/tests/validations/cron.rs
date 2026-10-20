use crate::validations::cron::validate_report_inputs;
use domain::dto::{SchedulePeriodMode, ScheduleReportType};

#[test]
fn validates_transaction_presets_and_inventory_recurrence() {
    assert!(
        validate_report_inputs(
            ScheduleReportType::Transaction,
            "0 0 1 * *",
            Some(SchedulePeriodMode::PreviousMonth),
            None
        )
        .is_ok()
    );
    assert!(
        validate_report_inputs(
            ScheduleReportType::Transaction,
            "0 0 16 * *",
            Some(SchedulePeriodMode::FirstHalfMonth),
            None
        )
        .is_ok()
    );
    assert!(
        validate_report_inputs(
            ScheduleReportType::Inventory,
            "*/5 9-17 1,16 * 1-5",
            None,
            Some("123456789")
        )
        .is_ok()
    );
}

#[test]
fn rejects_incompatible_report_inputs_and_transaction_cron_fields() {
    for (expression, mode, inventory_id) in [
        ("0 0 16 * *", Some(SchedulePeriodMode::PreviousMonth), None),
        ("0 0 1 * *", Some(SchedulePeriodMode::FirstHalfMonth), None),
        ("0 0 1 2 *", Some(SchedulePeriodMode::PreviousMonth), None),
        ("0 0 1 * 1", Some(SchedulePeriodMode::PreviousMonth), None),
        ("0 0 1 * *", None, None),
        (
            "0 0 1 * *",
            Some(SchedulePeriodMode::PreviousMonth),
            Some("1"),
        ),
        ("0 0 1 * *", None, Some("1")),
    ] {
        assert!(
            validate_report_inputs(
                ScheduleReportType::Transaction,
                expression,
                mode,
                inventory_id
            )
            .is_err()
        );
    }
    for inventory_id in [None, Some("0"), Some("01"), Some("abc")] {
        assert!(
            validate_report_inputs(
                ScheduleReportType::Inventory,
                "0 9 * * *",
                None,
                inventory_id
            )
            .is_err()
        );
    }
    assert!(
        validate_report_inputs(
            ScheduleReportType::Inventory,
            "0 9 * * *",
            Some(SchedulePeriodMode::PreviousMonth),
            Some("12")
        )
        .is_err()
    );
}

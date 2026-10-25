use crate::services;
use domain::dto::{SchedulePeriodMode, ScheduleReportType};

#[test]
fn validates_transaction_presets_and_inventory_recurrence() {
    assert!(
        services::cron::config::validate_report_inputs(
            ScheduleReportType::Transaction,
            "0 0 1 * *",
            Some(SchedulePeriodMode::PreviousMonth),
            None
        )
        .is_ok()
    );
    assert!(
        services::cron::config::validate_report_inputs(
            ScheduleReportType::Transaction,
            "0 0 16 * *",
            Some(SchedulePeriodMode::FirstHalfMonth),
            None
        )
        .is_ok()
    );
    assert!(
        services::cron::config::validate_report_inputs(
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
            services::cron::config::validate_report_inputs(
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
            services::cron::config::validate_report_inputs(
                ScheduleReportType::Inventory,
                "0 9 * * *",
                None,
                inventory_id
            )
            .is_err()
        );
    }
    assert!(
        services::cron::config::validate_report_inputs(
            ScheduleReportType::Inventory,
            "0 9 * * *",
            Some(SchedulePeriodMode::PreviousMonth),
            Some("12")
        )
        .is_err()
    );
}

#[test]
fn create_validation_keeps_request_errors_before_report_compatibility_errors() {
    let mut request = domain::dto::CreateCronConfiguration {
        name: String::new(),
        receiver_email: "invalid".into(),
        report_type: ScheduleReportType::Transaction,
        cron_expression: "bad".into(),
        parameter: "invalid".into(),
        enabled: false,
    };
    let Err(error) = crate::validations::cron::configs::validate_create(request.clone()) else {
        panic!("name must be rejected");
    };
    assert_eq!(error.to_string(), "name must contain 1 to 120 characters");
    request.name = " Reports ".into();
    let Err(error) = crate::validations::cron::configs::validate_create(request.clone()) else {
        panic!("email must be rejected");
    };
    assert_eq!(
        error.to_string(),
        "receiver_email must be a valid email address"
    );
    request.receiver_email = " reports@example.test ".into();
    let checked = crate::validations::cron::configs::validate_create(request.clone())
        .expect("valid request fields");
    let error = super::configuration_input(checked)
        .err()
        .expect("parameter rejected before cron");
    assert_eq!(
        error.to_string(),
        "Transaction parameter must be PREVIOUS_MONTH or FIRST_HALF_MONTH"
    );
    request.parameter = "PREVIOUS_MONTH".into();
    let checked = crate::validations::cron::configs::validate_create(request.clone())
        .expect("valid request fields");
    let error = super::configuration_input(checked)
        .err()
        .expect("cron rejected");
    assert_eq!(
        error.to_string(),
        "cron_expression must be a valid five-field cron expression"
    );
    request.cron_expression = " 0  9  1 * * ".into();
    let checked =
        crate::validations::cron::configs::validate_create(request).expect("valid request fields");
    let input = super::configuration_input(checked).expect("valid compatible schedule");
    assert_eq!(input.name, "Reports");
    assert_eq!(input.receiver_email, "reports@example.test");
    assert_eq!(input.cron_expression, "0 9 1 * *");
}

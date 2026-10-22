use super::*;
use domain::dto::{PatchField, ScheduleReportType};

#[test]
fn presets_encode_bangkok_wall_clock_and_parameters() {
    let mut form = ScheduleForm {
        name: "  Monthly report  ".into(),
        email: "reader@example.com".into(),
        ..Default::default()
    };
    assert_eq!(
        form.create().expect("valid input").cron_expression,
        "0 0 1 * *"
    );
    form.set_cadence(Cadence::FirstHalf);
    assert_eq!(
        form.create().expect("valid input").cron_expression,
        "0 0 16 * *"
    );
    form.time = "18:45".into();
    assert_eq!(
        form.create().expect("valid input").cron_expression,
        "45 18 16 * *"
    );
    form.switch_type(ScheduleReportType::Inventory);
    assert_eq!(form.parameter, "");
    form.parameter = "123".into();
    assert_eq!(
        form.create().expect("valid input").cron_expression,
        "0 9 * * *"
    );
    let patch = form.patch().expect("valid input");
    assert_eq!(
        patch.report_type,
        PatchField::Set(ScheduleReportType::Inventory)
    );
    assert_eq!(patch.parameter, PatchField::Set("123".into()));
    assert_eq!(patch.enabled, PatchField::Set(false));
    form.switch_type(ScheduleReportType::Transaction);
    assert_eq!(form.parameter, "PREVIOUS_MONTH");
}

#[test]
fn fields_reject_invalid_values_without_rejecting_custom_semantics_locally() {
    let mut form = ScheduleForm::default();
    assert!(
        form.create()
            .expect_err("invalid input")
            .iter()
            .any(|e| e.field == "name")
    );
    form.name = " x ".into();
    form.email = "reader@example.com".into();
    assert_eq!(form.create().expect("valid input").name, "x");
    form.name = "é".repeat(120);
    assert!(form.create().is_ok());
    form.name.push('é');
    assert!(form.create().is_err());
    form.name = "Inventory".into();
    form.switch_type(ScheduleReportType::Inventory);
    for value in ["", "0", "01", "-1", "1.0", "１２３", "123 "] {
        form.parameter = value.into();
        assert!(form.create().is_err(), "{value:?}");
    }
    form.parameter = "123".into();
    form.cadence = Cadence::Custom;
    form.custom = "*/5 9-17 * * MON-FRI".into();
    assert!(form.create().is_ok());
    form.custom = "0 0 * *".into();
    assert!(
        form.create()
            .expect_err("invalid input")
            .iter()
            .any(|e| e.field == "cron")
    );
    form.cadence = Cadence::Daily;
    form.time = "24:00".into();
    assert!(
        form.create()
            .expect_err("invalid input")
            .iter()
            .any(|e| e.field == "time")
    );
}

#[test]
fn unmatched_saved_cron_is_preserved_even_when_other_fields_change() {
    let record = domain::dto::CronConfigurationDTO {
        id: "schedule".into(),
        name: "Saved".into(),
        report_type: ScheduleReportType::Inventory,
        cron_expression: "  00 09 * * MON-FRI  ".into(),
        receiver_email: "reader@example.com".into(),
        parameter: "123".into(),
        enabled: false,
        schedule_timezone: "Asia/Bangkok".into(),
        created_at: String::new(),
        updated_at: String::new(),
    };
    let mut form = ScheduleForm::from_record(&record);
    assert_eq!(form.cadence, Cadence::Custom);
    form.name = "Renamed".into();
    assert_eq!(
        form.create().expect("valid input").cron_expression,
        record.cron_expression
    );
    assert_eq!(
        form.preview().expect("valid input").cron_expression,
        record.cron_expression
    );
}

#[test]
fn schedules_start_disabled_and_inventory_weekly_monthly_keep_bangkok_time() {
    let mut form = ScheduleForm {
        name: "Inventory".into(),
        email: "reader@example.com".into(),
        ..Default::default()
    };
    assert!(!form.enabled, "new schedules require explicit enablement");
    form.switch_type(ScheduleReportType::Inventory);
    form.parameter = "123".into();
    form.time = "18:45".into();
    form.set_cadence(Cadence::Weekly);
    form.weekday = "1".into();
    assert_eq!(
        form.create().expect("weekly form").cron_expression,
        "45 18 * * 1"
    );
    form.set_cadence(Cadence::Monthly);
    form.month_day = "31".into();
    assert_eq!(
        form.create().expect("monthly form").cron_expression,
        "45 18 31 * *"
    );
    for invalid in ["0", "32", "1.5", "", "01"] {
        form.month_day = invalid.into();
        assert!(form.create().is_err(), "invalid monthly day {invalid:?}");
    }
    form.month_day = "1".into();
    form.set_cadence(Cadence::Weekly);
    for invalid in ["7", "-1", "", "01"] {
        form.weekday = invalid.into();
        assert!(form.create().is_err(), "invalid weekday {invalid:?}");
    }
}

#[test]
fn matching_saved_weekly_and_monthly_inventory_uses_controls_without_changing_cron() {
    for (expression, expected) in [
        ("45 18 * * 1", Cadence::Weekly),
        ("45 18 31 * *", Cadence::Monthly),
    ] {
        let record = domain::dto::CronConfigurationDTO {
            id: "schedule".into(),
            name: "Saved".into(),
            report_type: ScheduleReportType::Inventory,
            cron_expression: expression.into(),
            receiver_email: "reader@example.com".into(),
            parameter: "123".into(),
            enabled: false,
            schedule_timezone: "Asia/Bangkok".into(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        let form = ScheduleForm::from_record(&record);
        assert_eq!(form.cadence, expected);
        assert_eq!(form.time, "18:45");
        assert_eq!(
            form.create().expect("saved form").cron_expression,
            expression
        );
    }
}

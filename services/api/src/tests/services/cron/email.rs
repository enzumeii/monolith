use super::*;
#[test]
fn inventory_content_identifies_saved_inventory_and_fixed_sender() {
    let content = report_content(
        "recipient@example.test",
        Report::Inventory { id: "12345" },
        generated_at(),
    );
    assert_eq!(content.from, "reporting@enzumeii.com");
    assert_eq!(
        content.subject,
        "Inventory report - inventory ID 12345 - Generated: 2026-10-23 01:02:03 +07:00 (Asia/Bangkok)"
    );
    assert!(
        content
            .html
            .contains("Report generation date: 2026-10-23 01:02:03 +07:00 (Asia/Bangkok)")
    );
    assert!(content.html.contains("Report type: Inventory"));
    assert!(content.html.contains("Inventory ID: 12345"));
}
#[test]
fn transaction_content_keeps_inclusive_saved_bangkok_range() {
    let start = "2026-09-01T00:00:00+07:00";
    let end = "2026-09-30T23:59:59+07:00";
    let content = report_content(
        "recipient@example.test",
        Report::Transaction { start, end },
        generated_at(),
    );
    assert_eq!(
        content.subject,
        format!(
            "Transaction report - {start} through {end} (Asia/Bangkok, UTC+07:00) - Generated: 2026-10-23 01:02:03 +07:00 (Asia/Bangkok)"
        )
    );
    assert!(
        content
            .html
            .contains("Report generation date: 2026-10-23 01:02:03 +07:00 (Asia/Bangkok)")
    );
    assert!(content.html.contains("Report type: Transaction"));
    assert!(content.html.contains(&format!("Date range start: {start}")));
    assert!(
        content
            .html
            .contains(&format!("Date range end (inclusive): {end}"))
    );
    assert!(content.html.contains("Timezone: Asia/Bangkok (UTC+07:00)"));
}
#[test]
fn interpolated_html_is_escaped() {
    assert_eq!(escape_html("&<>\"'"), "&amp;&lt;&gt;&quot;&#39;");
    let content = report_content(
        "recipient@example.test",
        Report::Inventory { id: "<&>\"'" },
        generated_at(),
    );
    assert!(content.html.contains("&lt;&amp;&gt;&quot;&#39;"));
}

fn generated_at() -> chrono::NaiveDateTime {
    chrono::NaiveDate::from_ymd_opt(2026, 10, 22)
        .expect("valid test fixture")
        .and_hms_opt(18, 2, 3)
        .expect("valid test fixture")
}

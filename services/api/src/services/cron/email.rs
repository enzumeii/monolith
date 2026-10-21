#[derive(Clone, Copy)]
pub(crate) enum Report<'a> {
    Inventory { id: &'a str },
    Transaction { start: &'a str, end: &'a str },
}
pub(crate) struct ReportContent {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub html: String,
}

pub(crate) fn report_content(recipient: &str, report: Report<'_>) -> ReportContent {
    let (subject, html) = match report {
        Report::Inventory { id } => (
            format!("Inventory report - inventory ID {id}"),
            format!(
                "<p>Report type: Inventory</p><p>Inventory ID: {}</p><p>Your report is attached.</p>",
                escape_html(id)
            ),
        ),
        Report::Transaction { start, end } => (
            format!("Transaction report - {start} through {end} (Asia/Bangkok, UTC+07:00)"),
            format!(
                "<p>Report type: Transaction</p><p>Date range start: {}</p><p>Date range end (inclusive): {}</p><p>Timezone: Asia/Bangkok (UTC+07:00)</p><p>Your reports are attached.</p>",
                escape_html(start),
                escape_html(end)
            ),
        ),
    };
    ReportContent {
        from: "reporting@enzumeii.com".into(),
        to: recipient.into(),
        subject,
        html,
    }
}
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
#[path = "../../tests/services/cron/email.rs"]
mod tests;

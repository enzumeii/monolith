use chrono::{DateTime, SecondsFormat};
#[derive(Clone, Debug, PartialEq)]
pub struct ReportRequest {
    pub date_from: String,
    pub date_to: String,
}
pub fn date_range(start: &str, end: &str) -> Result<ReportRequest, String> {
    let parse = |value: &str| {
        let value = match value.len() {
            16 => format!("{value}:00+07:00"),
            19 => format!("{value}+07:00"),
            _ => return Err("Enter valid start and end dates and times".to_string()),
        };
        DateTime::parse_from_rfc3339(&value)
            .map_err(|_| "Enter valid start and end dates and times".to_string())
    };
    let start = parse(start)?;
    let end = parse(end)?;
    if start >= end {
        return Err("Start must be before end".into());
    }
    Ok(ReportRequest {
        date_from: start.to_rfc3339_opts(SecondsFormat::Secs, false),
        date_to: end.to_rfc3339_opts(SecondsFormat::Secs, false),
    })
}
#[cfg(test)]
#[path = "../../../tests/pages/reports/accounting/data.rs"]
mod tests;

use chrono::{DateTime, FixedOffset, NaiveDateTime, SecondsFormat};

pub fn bangkok_time(value: &str) -> String {
    DateTime::parse_from_rfc3339(value).map_or_else(
        |_| value.to_owned(),
        |time| {
            time.with_timezone(&bangkok())
                .format("%Y-%m-%d %H:%M:%S%.f +07:00")
                .to_string()
        },
    )
}

pub fn local_bound(value: &str) -> Result<Option<String>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    let normalized = if value.len() == 16 {
        format!("{value}:00")
    } else {
        value.to_owned()
    };
    let local = NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%dT%H:%M:%S%.f")
        .map_err(|_| "Enter a valid Bangkok date and time".to_string())?;
    let instant = local
        .and_local_timezone(bangkok())
        .single()
        .expect("fixed offset");
    Ok(Some(instant.to_rfc3339_opts(SecondsFormat::AutoSi, false)))
}

pub fn validate_bounds(start: Option<&str>, end: Option<&str>) -> Result<(), String> {
    let parse = |value: &str| {
        DateTime::parse_from_rfc3339(value)
            .map_err(|_| "Date filters must include a valid time and UTC offset".to_string())
    };
    let start = start.map(parse).transpose()?;
    let end = end.map(parse).transpose()?;
    if let (Some(start), Some(end)) = (start, end)
        && start >= end
    {
        return Err("Start must be before the exclusive end".into());
    }
    Ok(())
}

pub fn local_control(value: &str) -> String {
    DateTime::parse_from_rfc3339(value).map_or_else(
        |_| value.to_owned(),
        |time| {
            time.with_timezone(&bangkok())
                .format("%Y-%m-%dT%H:%M:%S%.f")
                .to_string()
        },
    )
}

fn bangkok() -> FixedOffset {
    FixedOffset::east_opt(7 * 3600).expect("Bangkok offset")
}

#[cfg(test)]
#[path = "../../tests/pages/scheduled_reports/time.rs"]
mod tests;

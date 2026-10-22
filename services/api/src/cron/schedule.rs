use chrono::{DateTime, Utc};
use chrono_tz::Asia::Bangkok;
use croner::Cron;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CronScheduleError {
    InvalidExpression,
    NoFutureOccurrence,
}

pub fn next_occurrence(
    expression: &str,
    after: DateTime<Utc>,
) -> Result<DateTime<Utc>, CronScheduleError> {
    let expression = canonical_expression(expression)?;
    let cron = Cron::from_str(&expression).map_err(|_| CronScheduleError::InvalidExpression)?;
    let after_bangkok = after.with_timezone(&Bangkok);
    cron.find_next_occurrence(&after_bangkok, false)
        .map(|occurrence| occurrence.with_timezone(&Utc))
        .map_err(|_| CronScheduleError::NoFutureOccurrence)
}

pub fn normalize_expression(expression: &str) -> Result<String, CronScheduleError> {
    let expression = canonical_expression(expression)?;
    Cron::from_str(&expression).map_err(|_| CronScheduleError::InvalidExpression)?;
    Ok(expression)
}

pub fn describe(expression: &str) -> Result<String, CronScheduleError> {
    let expression = normalize_expression(expression)?;
    let cron = Cron::from_str(&expression).map_err(|_| CronScheduleError::InvalidExpression)?;
    Ok(cron.describe())
}

pub fn next_occurrences(
    expression: &str,
    after: DateTime<Utc>,
    count: usize,
) -> Result<Vec<DateTime<Utc>>, CronScheduleError> {
    let expression = normalize_expression(expression)?;
    let cron = Cron::from_str(&expression).map_err(|_| CronScheduleError::InvalidExpression)?;
    let mut after_bangkok = after.with_timezone(&Bangkok);
    let mut occurrences = Vec::with_capacity(count);
    for _ in 0..count {
        after_bangkok = cron
            .find_next_occurrence(&after_bangkok, false)
            .map_err(|_| CronScheduleError::NoFutureOccurrence)?;
        occurrences.push(after_bangkok.with_timezone(&Utc));
    }
    Ok(occurrences)
}

fn canonical_expression(expression: &str) -> Result<String, CronScheduleError> {
    let fields = expression.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err(CronScheduleError::InvalidExpression);
    }
    Ok(fields.join(" "))
}

#[cfg(test)]
#[path = "../tests/cron/schedule.rs"]
mod tests;

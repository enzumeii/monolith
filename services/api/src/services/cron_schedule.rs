use chrono::{DateTime, Datelike, Months, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Asia::Bangkok;
use croner::Cron;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CronScheduleError {
    InvalidExpression,
    InvalidPeriod,
    NoFutureOccurrence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionPeriodMode {
    PreviousMonth,
    FirstHalfMonth,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TransactionWindow {
    pub start: NaiveDateTime,
    pub end_exclusive: NaiveDateTime,
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

pub fn transaction_window(
    mode: TransactionPeriodMode,
    scheduled_for: DateTime<Utc>,
) -> Result<TransactionWindow, CronScheduleError> {
    let local_date = scheduled_for.with_timezone(&Bangkok).date_naive();
    let month_start = NaiveDate::from_ymd_opt(local_date.year(), local_date.month(), 1)
        .ok_or(CronScheduleError::InvalidPeriod)?;
    let (start_date, end_date) = match mode {
        TransactionPeriodMode::PreviousMonth => (
            month_start
                .checked_sub_months(Months::new(1))
                .ok_or(CronScheduleError::InvalidPeriod)?,
            month_start,
        ),
        TransactionPeriodMode::FirstHalfMonth => (
            month_start,
            month_start
                .checked_add_days(chrono::Days::new(15))
                .ok_or(CronScheduleError::InvalidPeriod)?,
        ),
    };
    let start = bangkok_utc_naive(start_date.and_time(chrono::NaiveTime::MIN))?;
    let end_exclusive = bangkok_utc_naive(end_date.and_time(chrono::NaiveTime::MIN))?;
    Ok(TransactionWindow {
        start,
        end_exclusive,
    })
}

fn bangkok_utc_naive(local: NaiveDateTime) -> Result<NaiveDateTime, CronScheduleError> {
    Bangkok
        .from_local_datetime(&local)
        .single()
        .map(|instant| instant.with_timezone(&Utc).naive_utc())
        .ok_or(CronScheduleError::InvalidPeriod)
}

fn canonical_expression(expression: &str) -> Result<String, CronScheduleError> {
    let fields = expression.split_whitespace().collect::<Vec<_>>();
    if fields.len() != 5 {
        return Err(CronScheduleError::InvalidExpression);
    }
    Ok(fields.join(" "))
}

#[cfg(test)]
#[path = "../tests/services/cron_schedule.rs"]
mod tests;

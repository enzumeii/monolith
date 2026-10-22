use chrono::Timelike;
use domain::dto::{
    CreateCronConfiguration, CronConfigurationDTO, PatchCronConfiguration, PatchField,
    PreviewCronConfiguration, ScheduleReportType,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cadence {
    PreviousMonth,
    FirstHalf,
    Daily,
    Weekly,
    Monthly,
    Custom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScheduleForm {
    pub name: String,
    pub email: String,
    pub report_type: ScheduleReportType,
    pub parameter: String,
    pub enabled: bool,
    pub cadence: Cadence,
    pub time: String,
    pub weekday: String,
    pub month_day: String,
    pub custom: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldError {
    pub field: &'static str,
    pub message: String,
}

impl ScheduleForm {
    pub fn from_record(record: &CronConfigurationDTO) -> Self {
        let mut form = Self {
            name: record.name.clone(),
            email: record.receiver_email.clone(),
            report_type: record.report_type,
            parameter: record.parameter.clone(),
            enabled: record.enabled,
            cadence: Cadence::Custom,
            time: "00:00".into(),
            weekday: "1".into(),
            month_day: "1".into(),
            custom: record.cron_expression.clone(),
        };
        let fields = record.cron_expression.split(' ').collect::<Vec<_>>();
        if fields.len() == 5 {
            let cadence = match (record.report_type, record.parameter.as_str(), &fields[2..]) {
                (ScheduleReportType::Transaction, "PREVIOUS_MONTH", ["1", "*", "*"]) => {
                    Some(Cadence::PreviousMonth)
                }
                (ScheduleReportType::Transaction, "FIRST_HALF_MONTH", ["16", "*", "*"]) => {
                    Some(Cadence::FirstHalf)
                }
                (ScheduleReportType::Inventory, _, ["*", "*", "*"]) => Some(Cadence::Daily),
                (ScheduleReportType::Inventory, _, ["*", "*", weekday])
                    if canonical_number(weekday, 0, 6) =>
                {
                    Some(Cadence::Weekly)
                }
                (ScheduleReportType::Inventory, _, [day, "*", "*"])
                    if canonical_number(day, 1, 31) =>
                {
                    Some(Cadence::Monthly)
                }
                _ => None,
            };
            if let (Some(cadence), Ok(minute), Ok(hour)) =
                (cadence, fields[0].parse::<u8>(), fields[1].parse::<u8>())
                && minute < 60
                && hour < 24
                && format!("{minute} {hour} {} {} {}", fields[2], fields[3], fields[4])
                    == record.cron_expression
            {
                form.cadence = cadence;
                form.time = format!("{hour:02}:{minute:02}");
                if cadence == Cadence::Weekly {
                    form.weekday = fields[4].into();
                }
                if cadence == Cadence::Monthly {
                    form.month_day = fields[2].into();
                }
            }
        }
        form
    }

    pub fn switch_type(&mut self, report_type: ScheduleReportType) {
        if self.report_type == report_type {
            return;
        }
        self.report_type = report_type;
        self.custom.clear();
        match report_type {
            ScheduleReportType::Transaction => {
                self.parameter = "PREVIOUS_MONTH".into();
                self.cadence = Cadence::PreviousMonth;
                self.time = "00:00".into();
            }
            ScheduleReportType::Inventory => {
                self.parameter.clear();
                self.cadence = Cadence::Daily;
                self.time = "09:00".into();
            }
        }
    }

    pub fn set_cadence(&mut self, cadence: Cadence) {
        if cadence == Cadence::Custom && self.cadence != Cadence::Custom {
            self.custom = self.expression().unwrap_or_default();
        }
        self.cadence = cadence;
        match cadence {
            Cadence::PreviousMonth => self.parameter = "PREVIOUS_MONTH".into(),
            Cadence::FirstHalf => self.parameter = "FIRST_HALF_MONTH".into(),
            _ => {}
        }
    }

    pub fn create(&self) -> Result<CreateCronConfiguration, Vec<FieldError>> {
        let mut errors = Vec::new();
        if !(1..=120).contains(&self.name.trim().chars().count()) {
            errors.push(field_error(
                "name",
                "Enter a name containing 1 to 120 characters",
            ));
        }
        if self.email.trim().is_empty() {
            errors.push(field_error("email", "Enter a receiver email address"));
        }
        match self.report_type {
            ScheduleReportType::Inventory
                if !crate::api::valid_location_id(&self.parameter)
                    || self.parameter.starts_with('0') =>
            {
                errors.push(field_error(
                    "parameter",
                    "Enter a positive numeric inventory ID without leading zeros",
                ));
            }
            ScheduleReportType::Transaction
                if !matches!(
                    self.parameter.as_str(),
                    "PREVIOUS_MONTH" | "FIRST_HALF_MONTH"
                ) =>
            {
                errors.push(field_error(
                    "parameter",
                    "Choose a transaction period policy",
                ));
            }
            _ => {}
        }
        let expression = match self.expression() {
            Ok(expression) => expression,
            Err(error) => {
                errors.push(error);
                String::new()
            }
        };
        if !errors.is_empty() {
            return Err(errors);
        }
        Ok(CreateCronConfiguration {
            name: self.name.trim().into(),
            receiver_email: self.email.trim().into(),
            report_type: self.report_type,
            cron_expression: expression,
            parameter: self.parameter.clone(),
            enabled: self.enabled,
        })
    }

    pub fn patch(&self) -> Result<PatchCronConfiguration, Vec<FieldError>> {
        let input = self.create()?;
        Ok(PatchCronConfiguration {
            name: PatchField::Set(input.name),
            receiver_email: PatchField::Set(input.receiver_email),
            report_type: PatchField::Set(input.report_type),
            cron_expression: PatchField::Set(input.cron_expression),
            parameter: PatchField::Set(input.parameter),
            enabled: PatchField::Set(input.enabled),
        })
    }

    pub fn preview(&self) -> Result<PreviewCronConfiguration, Vec<FieldError>> {
        let input = self.create()?;
        Ok(PreviewCronConfiguration {
            report_type: input.report_type,
            cron_expression: input.cron_expression,
            parameter: input.parameter,
        })
    }

    fn expression(&self) -> Result<String, FieldError> {
        if self.cadence == Cadence::Custom {
            if self.custom.split_whitespace().count() != 5 {
                return Err(field_error(
                    "cron",
                    "Enter a five-field cron expression (minute hour day month weekday)",
                ));
            }
            return Ok(self.custom.clone());
        }
        let time = chrono::NaiveTime::parse_from_str(&self.time, "%H:%M")
            .ok()
            .filter(|_| self.time.len() == 5)
            .ok_or_else(|| field_error("time", "Enter a Bangkok time in HH:mm format"))?;
        let day = match self.cadence {
            Cadence::PreviousMonth => "1",
            Cadence::FirstHalf => "16",
            Cadence::Monthly => {
                if !canonical_number(&self.month_day, 1, 31) {
                    return Err(field_error("month-day", "Choose a day between 1 and 31"));
                }
                &self.month_day
            }
            _ => "*",
        };
        let weekday = if self.cadence == Cadence::Weekly {
            if !canonical_number(&self.weekday, 0, 6) {
                return Err(field_error("weekday", "Choose a weekday"));
            }
            self.weekday.as_str()
        } else {
            "*"
        };
        Ok(format!(
            "{} {} {day} * {weekday}",
            time.minute(),
            time.hour()
        ))
    }
}

impl Default for ScheduleForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            email: String::new(),
            report_type: ScheduleReportType::Transaction,
            parameter: "PREVIOUS_MONTH".into(),
            enabled: false,
            cadence: Cadence::PreviousMonth,
            time: "00:00".into(),
            weekday: "1".into(),
            month_day: "1".into(),
            custom: String::new(),
        }
    }
}

pub fn cadence_label(record: &CronConfigurationDTO) -> String {
    let form = ScheduleForm::from_record(record);
    match form.cadence {
        Cadence::PreviousMonth => {
            format!("Monthly on day 1 at {} Bangkok · previous month", form.time)
        }
        Cadence::FirstHalf => format!(
            "Monthly on day 16 at {} Bangkok · first half of month",
            form.time
        ),
        Cadence::Daily => format!("Daily at {} Bangkok", form.time),
        Cadence::Weekly => format!(
            "Every {} at {} Bangkok",
            weekday_label(&form.weekday),
            form.time
        ),
        Cadence::Monthly => format!("Monthly on day {} at {} Bangkok", form.month_day, form.time),
        Cadence::Custom => format!("Custom cadence · {}", record.cron_expression),
    }
}

pub fn backend_field(message: &str) -> &'static str {
    if message.contains("name") {
        "name"
    } else if message.contains("receiver_email") {
        "email"
    } else if message.contains("parameter")
        || message.contains("inventory_id")
        || message.contains("period_mode")
    {
        "parameter"
    } else if message.contains("cron") {
        "cron"
    } else {
        "summary"
    }
}

pub fn weekday_label(value: &str) -> &'static str {
    match value {
        "0" => "Sunday",
        "1" => "Monday",
        "2" => "Tuesday",
        "3" => "Wednesday",
        "4" => "Thursday",
        "5" => "Friday",
        "6" => "Saturday",
        _ => "unknown weekday",
    }
}

fn canonical_number(value: &str, minimum: u8, maximum: u8) -> bool {
    value
        .parse::<u8>()
        .is_ok_and(|number| (minimum..=maximum).contains(&number) && number.to_string() == value)
}

fn field_error(field: &'static str, message: &str) -> FieldError {
    FieldError {
        field,
        message: message.into(),
    }
}

#[cfg(test)]
#[path = "../../tests/pages/scheduled_reports/data.rs"]
mod tests;

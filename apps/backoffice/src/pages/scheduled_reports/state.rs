pub use crate::components::request_state::{RequestState, use_owner as use_schedule_owner};
use domain::dto::{ListCronConfigurationsQuery, ScheduleReportType};

#[derive(Clone, Debug, PartialEq)]
pub struct Filters {
    pub report_type: Option<ScheduleReportType>,
    pub enabled: Option<bool>,
    pub page: u32,
    pub per_page: u32,
}
impl Filters {
    pub fn parse(query: &str) -> Self {
        let mut filters = Self::default();
        let mut url = reqwest::Url::parse("https://backoffice.invalid/").expect("static URL");
        url.set_query(Some(query.trim_start_matches('?')));
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "report_type" => filters.report_type = report_type(&value),
                "enabled" => filters.enabled = value.parse().ok(),
                "page" => filters.page = value.parse().ok().filter(|page| *page > 0).unwrap_or(1),
                "per_page" => {
                    filters.per_page = value
                        .parse()
                        .ok()
                        .filter(|size| (1..=100).contains(size))
                        .unwrap_or(25);
                }
                _ => {}
            }
        }
        filters
    }
    pub fn query(&self) -> String {
        let mut url = reqwest::Url::parse("https://backoffice.invalid/").expect("static URL");
        {
            let mut query = url.query_pairs_mut();
            if let Some(kind) = self.report_type {
                query.append_pair(
                    "report_type",
                    match kind {
                        ScheduleReportType::Transaction => "TRANSACTION",
                        ScheduleReportType::Inventory => "INVENTORY",
                    },
                );
            }
            if let Some(enabled) = self.enabled {
                query.append_pair("enabled", if enabled { "true" } else { "false" });
            }
            query.append_pair("page", &self.page.to_string());
            query.append_pair("per_page", &self.per_page.to_string());
        }
        url.query().unwrap_or_default().into()
    }
    pub fn request(&self) -> ListCronConfigurationsQuery {
        ListCronConfigurationsQuery {
            report_type: self.report_type,
            enabled: self.enabled,
            page: Some(self.page),
            per_page: Some(self.per_page),
        }
    }
    pub fn with_type(&self, value: &str) -> Self {
        Self {
            report_type: report_type(value),
            page: 1,
            ..self.clone()
        }
    }
}
impl Default for Filters {
    fn default() -> Self {
        Self {
            report_type: None,
            enabled: None,
            page: 1,
            per_page: 25,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SaveRecovery {
    pub failure: Option<crate::api::ApiError>,
    pub reload_required: bool,
}
impl SaveRecovery {
    pub fn save_failed(&mut self, error: crate::api::ApiError) {
        self.reload_required |= error.status == 409;
        self.failure = Some(error);
    }
    pub fn transient_failed(&mut self, error: crate::api::ApiError) {
        self.failure = Some(error);
    }
    pub fn edited(&mut self) {
        self.failure = None;
    }
    pub fn loaded(&mut self) {
        *self = Self::default();
    }
}

fn report_type(value: &str) -> Option<ScheduleReportType> {
    match value {
        "TRANSACTION" => Some(ScheduleReportType::Transaction),
        "INVENTORY" => Some(ScheduleReportType::Inventory),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/pages/scheduled_reports/state.rs"]
mod tests;

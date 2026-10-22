use super::ApiResult;
use domain::dto::{ListReportResultsQuery, ReportResultDTO};
use reqwest::Method;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ReportResultListResponse {
    pub items: Vec<ReportResultDTO>,
    pub page: u32,
    pub per_page: u32,
    pub has_next: bool,
}

pub async fn list_reports(query: ListReportResultsQuery) -> ApiResult<ReportResultListResponse> {
    super::decode(super::request(Method::GET, "/admin/cron/reports")?.query(&query)).await
}

pub async fn get_report(id: &str) -> ApiResult<ReportResultDTO> {
    super::decode(super::uuid_request(Method::GET, "/admin/cron/reports", id)?).await
}

#[cfg(test)]
#[path = "../tests/api/report_results.rs"]
mod tests;

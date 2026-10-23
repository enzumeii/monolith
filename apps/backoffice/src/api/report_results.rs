use super::ApiResult;
use domain::dto::{ListReportResultsQuery, ReportDownloadUrlDTO, ReportResultDTO};
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

pub async fn get_download_url(id: &str, filename: &str) -> ApiResult<ReportDownloadUrlDTO> {
    let mut request = super::uuid_request(Method::GET, "/admin/cron/reports", id)?
        .build()
        .map_err(|_| {
            super::ApiError::new(reqwest::StatusCode::BAD_REQUEST, "Invalid download request")
        })?;
    request
        .url_mut()
        .path_segments_mut()
        .map_err(|()| super::ApiError::new(reqwest::StatusCode::BAD_REQUEST, "Invalid API URL"))?
        .extend(["files", filename, "download-url"]);
    super::decode(reqwest::RequestBuilder::from_parts(
        reqwest::Client::new(),
        request,
    ))
    .await
}

#[cfg(test)]
#[path = "../tests/api/report_results.rs"]
mod tests;

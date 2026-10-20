use super::ApiResult;
use domain::dto::{AccountingReportQuery, AccountingReportResponse};
use reqwest::RequestBuilder;
pub async fn get_report(query: AccountingReportQuery) -> ApiResult<AccountingReportResponse> {
    super::decode(report_request(&query)?).await
}
fn report_request(query: &AccountingReportQuery) -> ApiResult<RequestBuilder> {
    super::request(reqwest::Method::GET, "/admin/transactions").map(|request| request.query(query))
}
#[cfg(test)]
#[path = "../tests/api/accounting.rs"]
mod tests;

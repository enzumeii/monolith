use crate::{error::AppError, services, state::AppState, validations};
use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
};
use domain::dto::{AccountingReportQuery, AccountingReportResponse};

pub async fn report(
    State(state): State<AppState>,
    query: Result<Query<AccountingReportQuery>, QueryRejection>,
) -> Result<Json<AccountingReportResponse>, AppError> {
    let range = validations::accounting::extract_accounting_query(query)?;
    Ok(Json(services::accounting::report(&state, range).await?))
}

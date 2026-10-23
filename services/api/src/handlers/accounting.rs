use super::dependencies::{paypal_client, shopify_client, stripe_client};
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
    let stripe = stripe_client(&state)?;
    let paypal = paypal_client(&state)?;
    let shopify = shopify_client(&state)?;
    Ok(Json(
        services::accounting::report(&state.pool, shopify, stripe, paypal, range).await?,
    ))
}

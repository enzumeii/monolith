use crate::{cron, error::AppError, validations};
use axum::http::StatusCode;
use domain::dto::CreateCronConfiguration;

pub(crate) fn validate_create(
    mut request: CreateCronConfiguration,
) -> Result<CreateCronConfiguration, AppError> {
    request.name = validations::common::name(&request.name)?;
    request.receiver_email = validations::common::email(
        &request.receiver_email,
        "receiver_email must be a valid email address",
    )?;
    Ok(request)
}

pub(crate) fn validate_expression(cron_expression: &str) -> Result<String, AppError> {
    cron::schedule::normalize_expression(cron_expression)
        .map_err(|_| invalid("cron_expression must be a valid five-field cron expression"))
}

pub(crate) fn validate_patch_shape(
    patch: &domain::dto::PatchCronConfiguration,
) -> Result<(), AppError> {
    use domain::dto::PatchField;
    let fields = [
        matches!(patch.name, PatchField::Missing),
        matches!(patch.report_type, PatchField::Missing),
        matches!(patch.cron_expression, PatchField::Missing),
        matches!(patch.receiver_email, PatchField::Missing),
        matches!(patch.parameter, PatchField::Missing),
        matches!(patch.enabled, PatchField::Missing),
    ];
    if fields.into_iter().all(|missing| missing) {
        return Err(invalid("PATCH body must contain at least one field"));
    }
    Ok(())
}
fn invalid(message: &'static str) -> AppError {
    AppError::new(StatusCode::BAD_REQUEST, message)
}

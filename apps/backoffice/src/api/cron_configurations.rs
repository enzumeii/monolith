use super::ApiResult;
use domain::dto::{
    CreateCronConfiguration, CronConfigurationDTO, CronConfigurationListResponse,
    CronConfigurationPreview, ListCronConfigurationsQuery, PatchCronConfiguration,
    PreviewCronConfiguration,
};
use reqwest::Method;

pub async fn list_configurations(
    query: ListCronConfigurationsQuery,
) -> ApiResult<CronConfigurationListResponse> {
    super::decode(super::request(Method::GET, "/admin/cron/configs")?.query(&query)).await
}

pub async fn get_configuration(id: &str) -> ApiResult<CronConfigurationDTO> {
    super::decode(super::uuid_request(Method::GET, "/admin/cron/configs", id)?).await
}

pub async fn create_configuration(
    input: CreateCronConfiguration,
) -> ApiResult<CronConfigurationDTO> {
    super::decode(super::request(Method::POST, "/admin/cron/configs")?.json(&input)).await
}

pub async fn update_configuration(
    id: &str,
    input: PatchCronConfiguration,
) -> ApiResult<CronConfigurationDTO> {
    super::decode(super::uuid_request(Method::PATCH, "/admin/cron/configs", id)?.json(&input)).await
}

pub async fn delete_configuration(id: &str) -> ApiResult<()> {
    super::send(super::uuid_request(
        Method::DELETE,
        "/admin/cron/configs",
        id,
    )?)
    .await?;
    Ok(())
}

pub async fn preview_configuration(
    query: PreviewCronConfiguration,
) -> ApiResult<CronConfigurationPreview> {
    super::decode(super::request(Method::GET, "/admin/cron/configs/preview")?.query(&query)).await
}

#[cfg(test)]
#[path = "../tests/api/cron_configurations.rs"]
mod tests;

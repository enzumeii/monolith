use crate::{
    client::s3::S3Client, error::AppError, repositories::conventions as repo,
    storage::convention_logo as storage,
};
use domain::dto::ConventionDTO;
use sqlx::PgPool;
use uuid::Uuid;
pub(crate) async fn upload(
    pool: &PgPool,
    s3: &S3Client,
    id: Uuid,
    logo: storage::Logo,
) -> Result<ConventionDTO, AppError> {
    let asset = Uuid::new_v4();
    storage::put(s3, id, asset, logo).await?;
    let mut tx = pool.begin().await?;
    let saved = repo::update_logo(&mut tx, id, &storage::url(id, asset))
        .await?
        .ok_or_else(super::missing)?;
    tx.commit().await?;
    saved.into_dto()
}
pub(crate) async fn current(pool: &PgPool, id: Uuid, asset: Uuid) -> Result<(), AppError> {
    let record = super::get(pool, id).await?;
    if record.logo_url.as_deref() != Some(&storage::url(id, asset)) {
        return Err(super::missing());
    }
    Ok(())
}

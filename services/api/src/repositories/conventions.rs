use crate::{
    error::AppError,
    models::conventions::{Convention, ConventionInput, ConventionVariant, method_values},
};
use domain::dto::ConventionVariantInput;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;
pub(crate) async fn list(
    pool: &PgPool,
    page: u32,
    per_page: u32,
    enabled: Option<bool>,
) -> Result<Vec<Convention>, AppError> {
    Ok(sqlx::query_as("SELECT * FROM conventions WHERE ($1::bool IS NULL OR preorder_enabled=$1) ORDER BY created_at DESC,id DESC LIMIT $2 OFFSET $3").bind(enabled).bind(i64::from(per_page)+1).bind(i64::from(page-1)*i64::from(per_page)).fetch_all(pool).await?)
}
pub(crate) async fn find(
    pool: &PgPool,
    id: Uuid,
    enabled_only: bool,
) -> Result<Option<Convention>, AppError> {
    Ok(
        sqlx::query_as("SELECT * FROM conventions WHERE id=$1 AND (NOT $2 OR preorder_enabled)")
            .bind(id)
            .bind(enabled_only)
            .fetch_optional(pool)
            .await?,
    )
}
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "Shared enabled-only lookup for the preorder flow")
)]
pub(crate) async fn enabled_slug(
    pool: &PgPool,
    slug: &str,
) -> Result<Option<Convention>, AppError> {
    Ok(
        sqlx::query_as("SELECT * FROM conventions WHERE slug=$1 AND preorder_enabled")
            .bind(slug)
            .fetch_optional(pool)
            .await?,
    )
}
pub(crate) async fn insert(pool: &PgPool, v: &ConventionInput) -> Result<Convention, AppError> {
    sqlx::query_as("INSERT INTO conventions(slug,name,logo_location,country_code,event_starts_on,event_ends_on,description_markdown,currency_code,payment_methods) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *").bind(&v.slug).bind(&v.name).bind(&v.logo_location).bind(&v.country_code).bind(v.event_starts_on).bind(v.event_ends_on).bind(&v.description_markdown).bind(&v.currency_code).bind(method_values(&v.payment_methods)).fetch_one(pool).await.map_err(conflict)
}
pub(crate) async fn lock(c: &mut PgConnection, id: Uuid) -> Result<Option<Convention>, AppError> {
    Ok(
        sqlx::query_as("SELECT * FROM conventions WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(c)
            .await?,
    )
}
pub(crate) async fn update(
    c: &mut PgConnection,
    id: Uuid,
    v: &ConventionInput,
) -> Result<Convention, AppError> {
    Ok(sqlx::query_as("UPDATE conventions SET name=$2,logo_location=$3,country_code=$4,event_starts_on=$5,event_ends_on=$6,description_markdown=$7,payment_methods=$8,preorder_enabled=$9,updated_at=GREATEST(CURRENT_TIMESTAMP,updated_at+INTERVAL '1 microsecond') WHERE id=$1 RETURNING *").bind(id).bind(&v.name).bind(&v.logo_location).bind(&v.country_code).bind(v.event_starts_on).bind(v.event_ends_on).bind(&v.description_markdown).bind(method_values(&v.payment_methods)).bind(v.preorder_enabled).fetch_one(c).await?)
}
pub(crate) async fn delete(c: &mut PgConnection, id: Uuid) -> Result<(), AppError> {
    sqlx::query("DELETE FROM conventions WHERE id=$1")
        .bind(id)
        .execute(c)
        .await?;
    Ok(())
}
pub(crate) async fn variants<'e, E>(c: E, id: Uuid) -> Result<Vec<ConventionVariant>, AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    Ok(sqlx::query_as("SELECT id,convention_id,shopify_product_id,shopify_variant_id,final_price::text,created_at,updated_at FROM convention_variants WHERE convention_id=$1 ORDER BY created_at,id").bind(id).fetch_all(c).await?)
}
pub(crate) async fn replace(
    c: &mut PgConnection,
    id: Uuid,
    items: &[ConventionVariantInput],
) -> Result<Vec<ConventionVariant>, AppError> {
    let ids: Vec<_> = items
        .iter()
        .map(|v| v.shopify_variant_id.as_str())
        .collect();
    sqlx::query("DELETE FROM convention_variants WHERE convention_id=$1 AND NOT(shopify_variant_id=ANY($2))").bind(id).bind(ids).execute(&mut *c).await?;
    for item in items {
        sqlx::query("INSERT INTO convention_variants(convention_id,shopify_product_id,shopify_variant_id,final_price) VALUES($1,$2,$3,$4::text::numeric) ON CONFLICT(convention_id,shopify_variant_id) DO UPDATE SET shopify_product_id=EXCLUDED.shopify_product_id,final_price=EXCLUDED.final_price,updated_at=CASE WHEN convention_variants.final_price IS DISTINCT FROM EXCLUDED.final_price OR convention_variants.shopify_product_id IS DISTINCT FROM EXCLUDED.shopify_product_id THEN GREATEST(CURRENT_TIMESTAMP,convention_variants.updated_at+INTERVAL '1 microsecond') ELSE convention_variants.updated_at END").bind(id).bind(&item.shopify_product_id).bind(&item.shopify_variant_id).bind(&item.final_price).execute(&mut *c).await?;
    }
    variants(c, id).await
}
fn conflict(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(ref db) = error
        && db.is_unique_violation()
        && db.constraint() == Some("conventions_slug_key")
    {
        return AppError::new(
            axum::http::StatusCode::CONFLICT,
            "Convention slug already exists",
        )
        .with_source(error);
    }
    error.into()
}

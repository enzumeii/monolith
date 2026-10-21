#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    config::Settings,
    state::{AppState, app_state},
};
use serde_json::json;

pub(crate) async fn state() -> AppState {
    let settings:Settings=serde_json::from_value(json!({
        "server":{"host":"127.0.0.1","port":0},
        "database":{"url":std::env::var("E2E_DATABASE_URL").unwrap_or_else(|_|"postgres://e2e:e2e@127.0.0.1:5433/e2e".into()),"max_connections":1,"min_connections":1},
        "browser":{"public_origin":"http://127.0.0.1:8084","insecure_local":true},
        "jwt":{"secret":"abababababababababababababababababababababababababababababababab"},
        "scheduler":{"enabled":false,"poll_interval_secs":1,"concurrency":1,"max_attempts":2,"retry_backoff_secs":1,"lease_secs":5,"max_payload_bytes":20_971_520,"max_email_bytes":20_971_520},
    })).unwrap();
    let pool = crate::repositories::create_pool(&settings).await.unwrap();
    sqlx::raw_sql("CREATE TEMP TABLE cron_configurations (LIKE public.cron_configurations INCLUDING ALL);
        CREATE TEMP TABLE cron_jobs (LIKE public.cron_jobs INCLUDING ALL);
        ALTER TABLE cron_jobs ADD FOREIGN KEY (cron_configuration_id) REFERENCES cron_configurations(id) ON DELETE CASCADE;
        CREATE TEMP TABLE report_results (LIKE public.report_results INCLUDING ALL);").execute(&pool).await.unwrap();
    app_state(pool, settings).unwrap()
}
pub(crate) fn input() -> ConfigurationInput {
    ConfigurationInput {
        name: "Owned".into(),
        report_type: domain::dto::ScheduleReportType::Inventory,
        cron_expression: "* * * * *".into(),
        receiver_email: "reports@example.test".into(),
        parameter: "123".into(),
        enabled: true,
    }
}
#[tokio::test]
async fn conditional_update_rejects_stale_snapshot_without_explicit_transaction() {
    let state = state().await;
    let config = insert(&state.pool, &input()).await.unwrap();
    let mut edited = input();
    edited.name = "Changed".into();
    let updated = update(&state.pool, config.id, config.updated_at, &edited)
        .await
        .unwrap()
        .unwrap();
    assert!(updated.updated_at > config.updated_at);
    assert!(
        update(&state.pool, config.id, config.updated_at, &input())
            .await
            .unwrap()
            .is_none()
    );
    let filter = ConfigurationFilter {
        report_type: Some(edited.report_type),
        enabled: Some(true),
        page: 1,
        per_page: 1,
    };
    assert_eq!(list(&state.pool, filter).await.unwrap().len(), 1);
    assert_eq!(
        find(&state.pool, config.id).await.unwrap().unwrap().name,
        "Changed"
    );
    state.pool.close().await;
}
#[tokio::test]
async fn delete_cascades_jobs_but_retains_cancelled_independent_audit() {
    let state = state().await;
    let config = insert(&state.pool, &input()).await.unwrap();
    crate::repositories::cron_jobs::enqueue(
        &mut state.pool.acquire().await.unwrap(),
        config.id,
        chrono::Utc::now().naive_utc(),
    )
    .await
    .unwrap();
    let claim = crate::cron::job::claim(&state).await.unwrap().unwrap();
    crate::services::cron::config::delete(&state.pool, config.id)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cron_jobs")
            .fetch_one(&state.pool)
            .await
            .unwrap(),
        0
    );
    let audit = crate::repositories::report_results::find(&state.pool, claim.job.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(audit.status, "FAILED");
    assert!(audit.error.unwrap().contains("Schedule deleted"));
    assert!(
        !crate::repositories::cron_jobs::renew(
            &state.pool,
            claim.job.id,
            claim.token,
            crate::cron::job::lease_until(&state)
        )
        .await
        .unwrap()
    );
    state.pool.close().await;
}

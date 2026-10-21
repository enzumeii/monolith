#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    client::tests::{Fixture, Reply},
    repositories::cron_configurations::tests::state,
};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn expired_or_missing_email_checkpoint_never_resends() {
    let (mut state, directory) = state().await;
    let fixture = Fixture::new(vec![Reply::json(json!({"id":"must-not-send"}))]).await;
    state.resend = Ok(Some(Arc::new(
        crate::client::resend::tests::client_with_url(fixture.url.clone()),
    )));
    let mut settings = (*state.settings).clone();
    settings.shipping_sender = Some(crate::config::ShippingSender {
        name: "Synthetic".into(),
        phone: "000000".into(),
        email: "sender@example.test".into(),
        address: "Synthetic".into(),
        sub_district: "Synthetic".into(),
        district: "Synthetic".into(),
        province: "Synthetic".into(),
        postcode: "10000".into(),
        country_code: "TH".into(),
    });
    state.settings = Arc::new(settings);
    let id = Uuid::new_v4();
    let prepare = || {
        vec![report_storage::GeneratedFile {
            filename: "inventory.csv".into(),
            content_type: report_storage::CSV_CONTENT_TYPE.into(),
            bytes: b"header\r\n".to_vec(),
        }]
    };
    let prepared =
        report_storage::prepare(&directory, id, chrono::Utc::now().naive_utc(), prepare())
            .await
            .unwrap();
    assert!(
        report(&state, id, "recipient@example.test", prepared, true)
            .await
            .is_err()
    );
    report_storage::mail_intent(
        &directory,
        id,
        &MailIntent {
            from: "sender@example.test".into(),
            to: "recipient@example.test".into(),
            subject: "Report".into(),
            html: "Report".into(),
            first_attempt_at: chrono::Utc::now().naive_utc() - chrono::Duration::hours(25),
        },
    )
    .await
    .unwrap();
    let prepared = report_storage::load(&directory, id).await.unwrap().unwrap();
    assert!(
        report(&state, id, "recipient@example.test", prepared, true)
            .await
            .is_err()
    );
    assert_eq!(fixture.requests().len(), 0);
    state.pool.close().await;
    tokio::fs::remove_dir_all(directory).await.unwrap();
}

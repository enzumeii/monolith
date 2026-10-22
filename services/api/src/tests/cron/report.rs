#![allow(clippy::unwrap_used)]
use super::*;
use std::{future::Future, task::Poll};

#[tokio::test]
async fn cancelling_active_csv_encoding_stops_at_partial_serialized_output() {
    let mut csv = String::new();
    let rows = (0..100_000).map(|index| vec![format!("=row,{index}")]);
    let mut encoding = Box::pin(encode_csv(&mut csv, &["text"], rows, &[], usize::MAX));
    std::future::poll_fn(|cx| {
        assert!(encoding.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    drop(encoding);
    assert_eq!(csv.lines().count(), 33);
    assert!(csv.ends_with("\"'=row,31\"\r\n"));
    let stopped = csv.clone();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(csv, stopped);
}

#[tokio::test]
async fn csv_payload_limit_stops_before_allocating_an_oversized_cell() {
    let mut csv = String::new();
    let oversized = "x".repeat(1_000_000);
    let result = encode_csv(&mut csv, &["text"], [vec![oversized]].into_iter(), &[], 100).await;
    assert_eq!(
        axum::response::IntoResponse::into_response(result.unwrap_err()).status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(csv, "text\r\n");
    assert!(csv.capacity() < 100);
}

#[tokio::test]
async fn cooperative_inventory_output_matches_the_existing_shared_encoder() {
    let row = domain::dto::ProductReportRow {
        category: None,
        collection: Some("é".into()),
        sku: Some("-SKU".into()),
        title: "=A, \"quote\"\nline".into(),
        variant: None,
        cost: Some("-2.50".parse().unwrap()),
        cost_currency: Some("THB".into()),
        weight: None,
        unit: None,
        selling_price: "1.2300".parse().unwrap(),
        selling_price_currency: "THB".into(),
        quantity: -2,
        total_selling_price: "-2.4600".parse().unwrap(),
        total_weight: None,
    };
    let rows = vec![row; 100];
    let expected = report_generator::inventory::encode(&rows).into_bytes();
    let prepared = prepare(
        Uuid::new_v4(),
        ScheduleReportType::Inventory,
        ReportData::Inventory(rows),
        expected.len(),
    )
    .await
    .unwrap();
    assert_eq!(prepared.files[0].bytes, expected);
}

#[tokio::test]
async fn transaction_files_share_one_total_payload_budget() {
    let data = || {
        ReportData::Transaction(domain::dto::AccountingReportResponse {
            transactions: domain::dto::accounting::AccountingTransactions::default(),
            items: Vec::new(),
        })
    };
    let transaction_csv = report_generator::accounting::encode_transactions([]);
    let item_csv = report_generator::accounting::encode_items(&[]);
    let limit = transaction_csv.len() + item_csv.len();
    let files = encode(data(), limit).await.unwrap();
    assert_eq!(files[0].bytes, transaction_csv.as_bytes());
    assert_eq!(files[1].bytes, item_csv.as_bytes());
    let error = encode(data(), limit - 1).await.err().unwrap();
    assert_eq!(
        axum::response::IntoResponse::into_response(error).status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

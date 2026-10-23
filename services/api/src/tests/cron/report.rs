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

#[tokio::test]
async fn frozen_digest_is_part_of_every_storage_key() {
    let id = Uuid::new_v4();
    let prepared = prepare(
        id,
        ScheduleReportType::Inventory,
        ReportData::Inventory(vec![]),
        1024,
    )
    .await
    .unwrap();
    let checkpoint = &prepared.checkpoint;
    let file = &checkpoint.files[0];
    assert_eq!(
        file.key,
        format!(
            "{}/{}/{}",
            checkpoint.storage_prefix, file.sha256, file.filename
        )
    );
    assert_eq!(
        file.sha256,
        super::super::publication::digest(&prepared.files[0].bytes)
    );
    assert_eq!(file.size_bytes, prepared.files[0].bytes.len());
    assert!(checkpoint.storage_prefix.ends_with(&id.to_string()));
}

#[tokio::test]
async fn checkpoint_rejects_a_digest_that_does_not_match_its_frozen_key() {
    let mut prepared = prepare(
        Uuid::new_v4(),
        ScheduleReportType::Inventory,
        ReportData::Inventory(vec![]),
        1024,
    )
    .await
    .unwrap();
    prepared.checkpoint.files[0].sha256 = "a".repeat(64);
    assert!(prepared.checkpoint.validate().is_err());
}

#[tokio::test]
async fn scheduled_transaction_csv_preserves_actual_tag_array_bytes() {
    let mut row: domain::dto::AccountingTransaction = serde_json::from_value(serde_json::json!({
        "gateway":"STRIPE", "type":"SALE", "po_number":"po_fixture", "po_date":null,
        "transaction_date":"2026-10-01T00:00:00Z", "payment_reference":"ch_fixture", "description":"fixture",
        "shopify_order_id":12, "shopify_order_number":12, "customer_name":"Fixture", "customer_address":"", "customer_phone":0,
        "currency":"THB", "exchange_rate":1.0, "net_paid":98.0, "fee":2.0, "vat":0.0, "gross_paid":100.0,
        "shipping_fee":0.0, "order_adjusted":false, "shopify_order_tags":[]
    })).unwrap();
    row.shopify_order_tags = vec!["Convention".into(), "comma,quote\"".into(), "ไทย".into()];
    let expected = report_generator::accounting::encode_transactions([&row]);
    let report = domain::dto::AccountingReportResponse {
        transactions: domain::dto::accounting::AccountingTransactions {
            stripe: vec![row],
            paypal: vec![],
        },
        items: vec![],
    };
    let files = encode(ReportData::Transaction(report), 16_384)
        .await
        .unwrap();
    assert_eq!(files[0].bytes, expected.as_bytes());
    let csv = String::from_utf8(files[0].bytes.clone()).unwrap();
    assert!(csv.contains("shopify_order_tags"));
    assert!(csv.contains("Convention"));
    assert!(csv.contains("ไทย"));
    assert!(csv.contains("98"));
}

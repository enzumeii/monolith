#![allow(clippy::unwrap_used)]
use super::*;
use crate::{
    client::tests::{Fixture, Reply},
    services::{checkout::tests::checkout, common::client},
};
use serde_json::json;

#[test]
fn purchase_tags_fit_shopify_limit_and_preserve_short_convention_names() {
    let convention_id = Uuid::new_v4();
    let checkout_id = Uuid::new_v4();
    let short = "Convention 2026";
    let tags = purchase_tags(convention_id, short, checkout_id);
    assert_eq!(
        tags,
        vec![
            format!("cv:{convention_id}"),
            short.to_owned(),
            format!("co:{checkout_id}")
        ]
    );
    assert!(tags.iter().all(|tag| tag.chars().count() <= 40));
    for name in ["A".repeat(120), "งานไทย🦊".repeat(20)] {
        let tags = purchase_tags(convention_id, &name, checkout_id);
        assert_eq!(tags[1], name.chars().take(40).collect::<String>());
        assert_eq!(tags[1].chars().count(), 40);
        assert!(tags.iter().all(|tag| tag.chars().count() <= 40));
        let mut row = checkout();
        row.required_tag_hashes = tags
            .iter()
            .map(|tag| Sha256::digest(tag.as_bytes()).to_vec())
            .collect();
        assert!(tags_match(&row, &tags));
        let mut changed = tags;
        changed[1].push('X');
        assert!(!tags_match(&row, &changed));
    }
}

#[tokio::test]
async fn recovery_search_uses_only_exact_correlation_bound_to_original_purchase() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://fixture:fixture@127.0.0.1:1/fixture")
        .unwrap();
    pool.close().await;
    for prefix in ["co", "checkout"] {
        let mut row = checkout();
        row.draft_id = None;
        let tag = format!("{prefix}:{}", row.id);
        row.required_tag_hashes = vec![Sha256::digest(tag.as_bytes()).to_vec()];
        let shop = Fixture::new(vec![Reply::json(json!({"data":{"draftOrders":{
            "nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}
        }}}))])
        .await;
        let shopify = client(&shop);
        let clients = CheckoutClients {
            shopify: &shopify,
            stripe: None,
            paypal: None,
        };
        assert!(
            recover(&pool, &clients, &mut row, Uuid::new_v4())
                .await
                .is_err()
        );
        let requests = shop.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].2["operationName"], "DraftOrders");
        assert_eq!(requests[0].2["variables"]["query"], format!("tag:'{tag}'"));
        assert!(row.draft_id.is_none());
    }
    let mut row = checkout();
    row.draft_id = None;
    row.required_tag_hashes = vec![Sha256::digest(b"co:foreign").to_vec()];
    let shop = Fixture::new(vec![]).await;
    let shopify = client(&shop);
    let clients = CheckoutClients {
        shopify: &shopify,
        stripe: None,
        paypal: None,
    };
    assert!(recovery_tag(&row).is_none());
    assert!(
        recover(&pool, &clients, &mut row, Uuid::new_v4())
            .await
            .is_err()
    );
    assert_eq!(shop.requests().len(), 0);
}

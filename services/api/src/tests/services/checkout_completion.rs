#![allow(clippy::unwrap_used, clippy::too_many_lines)]
use super::*;
use crate::models::checkout_completion::{PaymentEnvironment, SnapshotLine};
use crate::{
    client::tests::{Fixture, Reply},
    services::common::{client, id, page},
};
use axum::response::IntoResponse;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;

pub(crate) async fn pool() -> PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(
            &std::env::var("E2E_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://e2e:e2e@127.0.0.1:5433/e2e".into()),
        )
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}
pub(crate) fn fixture() -> (Uuid, Snapshot, PaymentAttempt, VerifiedPayment) {
    let checkout = Uuid::new_v4();
    let draft = id(
        "DraftOrder",
        u64::from_be_bytes(checkout.as_bytes()[..8].try_into().unwrap()),
    );
    let amount = ExactMoney::new(24690, "THB", 2).unwrap();
    let snapshot = Snapshot {
        draft_id: draft.clone(),
        accepted: amount.clone(),
        lines: vec![SnapshotLine {
            variant_id: id("ProductVariant", 1),
            quantity: 2,
            unit_price: ExactMoney::new(12345, "THB", 2).unwrap(),
        }],
        email: "buyer@example.test".into(),
        phone: Some("000".into()),
        attributes: BTreeMap::from([("pickup".into(), "Convention desk".into())]),
        tags: vec!["CONVENTION:TEST".into(), "PORTAL".into()],
        reservation_until: Utc::now() + chrono::Duration::hours(1),
    };
    let reference = format!("cs_{checkout}");
    let attempt = PaymentAttempt {
        attempt_id: format!("attempt-{checkout}"),
        provider: Provider::Stripe,
        account: "acct_fixture".into(),
        environment: PaymentEnvironment::Test,
        reference: reference.clone(),
    };
    let payment = VerifiedPayment {
        checkout_id: checkout,
        draft_id: draft,
        attempt: attempt.clone(),
        event_id: "event-1".into(),
        state: PaymentFact::Paid {
            money: amount,
            aliases: vec![
                PaymentAlias {
                    kind: AliasKind::StripeSession,
                    value: reference,
                },
                PaymentAlias {
                    kind: AliasKind::StripePaymentIntent,
                    value: format!("pi_{checkout}"),
                },
                PaymentAlias {
                    kind: AliasKind::StripeCharge,
                    value: format!("ch_{checkout}"),
                },
            ],
        },
    };
    (checkout, snapshot, attempt, payment)
}
pub(crate) async fn seed(
    pool: &PgPool,
    checkout: Uuid,
    snapshot: &Snapshot,
    attempt: &PaymentAttempt,
) {
    repo::insert(
        &mut pool.acquire().await.unwrap(),
        checkout,
        snapshot,
        attempt,
        &Sha256::digest(b"fixture-only-guest-credential"),
        Utc::now() + chrono::Duration::hours(1),
    )
    .await
    .unwrap();
}
pub(crate) async fn cleanup(pool: &PgPool, id: Uuid) {
    sqlx::query("DELETE FROM checkout_completions WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}
fn bag(amount: &str) -> Value {
    json!({"presentmentMoney":{"amount":amount,"currencyCode":"THB"}})
}
fn line() -> Value {
    json!({"variant":{"id":"gid://shopify/ProductVariant/1"},"quantity":2,"originalUnitPriceSet":bag("123.45")})
}
fn draft(snapshot: &Snapshot, order: Option<&ShopifyId>) -> Value {
    json!({"id":snapshot.draft_id,"ready":true,"status":if order.is_some(){"COMPLETED"}else{"OPEN"},"order":order.map(|id|json!({"id":id})),"reserveInventoryUntil":snapshot.reservation_until,"email":snapshot.email,"phone":snapshot.phone,"customAttributes":[{"key":"pickup","value":"Convention desk"}],"tags":snapshot.tags,"taxesIncluded":true,"totalPriceSet":bag("246.90"),"totalShippingPriceSet":bag("0.00"),"paymentTerms":null,"lineItems":page(vec![line()],None)})
}
fn order(snapshot: &Snapshot, order: &ShopifyId, metadata: Option<Value>) -> Value {
    json!({"id":order,"displayFinancialStatus":"PAID","displayFulfillmentStatus":"UNFULFILLED","cancelledAt":null,"email":snapshot.email,"phone":snapshot.phone,"customAttributes":[{"key":"pickup","value":"Convention desk"}],"tags":snapshot.tags,"taxesIncluded":true,"totalPriceSet":bag("246.90"),"totalShippingPriceSet":bag("0.00"),"lineItems":page(vec![line()],None),"metafield":metadata.map(|v|json!({"value":v.to_string(),"compareDigest":"digest"}))})
}
fn reply(key: &str, value: Value) -> Reply {
    let mut data = serde_json::Map::new();
    data.insert(key.into(), value);
    Reply::json(json!({"data":data}))
}
fn complete_reply(snapshot: &Snapshot, order: &ShopifyId) -> Reply {
    reply(
        "draftOrderComplete",
        json!({"draftOrder":{"id":snapshot.draft_id,"order":{"id":order}},"userErrors":[]}),
    )
}
fn metadata_reply() -> Reply {
    reply(
        "metafieldsSet",
        json!({"metafields":[{"namespace":"convention_checkout","key":"payment_references","value":"{}"}],"userErrors":[]}),
    )
}
#[tokio::test]
async fn paid_same_draft_persists_aliases_order_metadata_and_caches_replay() {
    let pool = pool().await;
    let (checkout, snapshot, attempt, payment) = fixture();
    seed(&pool, checkout, &snapshot, &attempt).await;
    let order_id = id(
        "Order",
        u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
    );
    let metadata = payment_metadata(checkout, &payment).unwrap();
    let f = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, None)),
        complete_reply(&snapshot, &order_id),
        reply("order", order(&snapshot, &order_id, None)),
        metadata_reply(),
        reply("order", order(&snapshot, &order_id, Some(metadata.clone()))),
    ])
    .await;
    let result = apply_verified_payment(&pool, &client(&f), &payment)
        .await
        .unwrap();
    assert_eq!(result.completion_state, CompletionState::Complete);
    assert_eq!(result.order_id, Some(order_id.clone()));
    assert_eq!(
        f.requests()
            .iter()
            .filter(|r| r.2["operationName"] == "CompleteCheckoutDraft")
            .count(),
        1
    );
    assert_eq!(
        f.requests()[1].2["variables"]["id"],
        json!(snapshot.draft_id)
    );
    assert_eq!(f.requests()[1].2["variables"]["paymentPending"], false);
    assert!(
        !f.requests()[1].2["variables"]
            .to_string()
            .contains("notify")
    );
    let cached = apply_verified_payment(&pool, &client(&f), &payment)
        .await
        .unwrap();
    assert_eq!(cached.order_id, Some(order_id));
    assert_eq!(f.requests().len(), 5);
    let aliases = repo::aliases_for_checkout(&pool, checkout).await.unwrap();
    assert_eq!(aliases.len(), 3);
    cleanup(&pool, checkout).await;
}
#[tokio::test]
async fn eligibility_rejects_money_account_draft_aliases_and_late_payment_before_mutation() {
    let pool = pool().await;
    for scenario in 0..7 {
        let (checkout, mut snapshot, attempt, mut payment) = fixture();
        if scenario == 6 {
            snapshot.reservation_until = Utc::now() - chrono::Duration::seconds(1);
        }
        seed(&pool, checkout, &snapshot, &attempt).await;
        match scenario {
            0 => payment.attempt.account = "wrong".into(),
            1 => payment.draft_id = id("DraftOrder", 999),
            2 => {
                if let PaymentFact::Paid { money, .. } = &mut payment.state {
                    money.minor += 1;
                }
            }
            3 => {
                if let PaymentFact::Paid { money, .. } = &mut payment.state {
                    money.currency = "USD".into();
                }
            }
            4 => {
                if let PaymentFact::Paid { aliases, .. } = &mut payment.state {
                    aliases.clear();
                }
            }
            5 => {
                if let PaymentFact::Paid { aliases, .. } = &mut payment.state {
                    aliases.push(PaymentAlias {
                        kind: AliasKind::StripeCharge,
                        value: "another".into(),
                    });
                }
            }
            _ => {}
        }
        let f = Fixture::new(if scenario == 6 {
            vec![reply("draftOrder", draft(&snapshot, None))]
        } else {
            vec![]
        })
        .await;
        assert_eq!(
            apply_verified_payment(&pool, &client(&f), &payment)
                .await
                .unwrap_err()
                .into_response()
                .status(),
            axum::http::StatusCode::CONFLICT
        );
        assert!(
            f.requests()
                .iter()
                .all(|r| r.2["operationName"] != "CompleteCheckoutDraft")
        );
        cleanup(&pool, checkout).await;
    }
}
#[tokio::test]
async fn lost_mutation_response_reconciles_exact_order_without_another_completion() {
    let pool = pool().await;
    let (checkout, snapshot, attempt, payment) = fixture();
    seed(&pool, checkout, &snapshot, &attempt).await;
    let f = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, None)),
        Reply::status(503),
    ])
    .await;
    assert!(
        apply_verified_payment(&pool, &client(&f), &payment)
            .await
            .is_err()
    );
    let saved = repo::find(&pool, checkout).await.unwrap().unwrap();
    assert_eq!(saved.payment_state, PaymentState::Paid);
    assert!(saved.completion_intent);
    let order_id = id(
        "Order",
        u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
    );
    let metadata = payment_metadata(checkout, &payment).unwrap();
    let f2 = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, Some(&order_id))),
        reply("order", order(&snapshot, &order_id, Some(metadata))),
    ])
    .await;
    assert_eq!(
        apply_verified_payment(&pool, &client(&f2), &payment)
            .await
            .unwrap()
            .completion_state,
        CompletionState::Complete
    );
    assert!(
        f2.requests()
            .iter()
            .all(|r| r.2["operationName"] != "CompleteCheckoutDraft")
    );
    cleanup(&pool, checkout).await;
}
#[tokio::test]
async fn ambiguous_open_draft_is_not_completed_again_and_pending_never_downgrades_paid() {
    let pool = pool().await;
    let (checkout, snapshot, attempt, mut payment) = fixture();
    seed(&pool, checkout, &snapshot, &attempt).await;
    let f = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, None)),
        Reply::status(503),
        reply("draftOrder", draft(&snapshot, None)),
    ])
    .await;
    assert!(
        apply_verified_payment(&pool, &client(&f), &payment)
            .await
            .is_err()
    );
    payment.state = PaymentFact::Expired;
    let result = apply_verified_payment(&pool, &client(&f), &payment)
        .await
        .unwrap();
    assert_eq!(result.payment_state, PaymentState::Paid);
    assert_eq!(result.completion_state, CompletionState::Uncertain);
    assert_eq!(f.requests().len(), 3);
    cleanup(&pool, checkout).await;
}
#[tokio::test]
async fn metadata_fault_resumes_saved_order_and_final_mismatch_keeps_order_for_operator() {
    let pool = pool().await;
    for mismatch in [false, true] {
        let (checkout, snapshot, attempt, payment) = fixture();
        seed(&pool, checkout, &snapshot, &attempt).await;
        let order_id = id(
            "Order",
            u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
        );
        let mut inspected = order(&snapshot, &order_id, None);
        if mismatch {
            inspected["tags"] = json!(["PACKING"]);
        }
        let f = Fixture::new(vec![
            reply("draftOrder", draft(&snapshot, None)),
            complete_reply(&snapshot, &order_id),
            reply("order", inspected),
            Reply::status(503),
        ])
        .await;
        assert!(
            apply_verified_payment(&pool, &client(&f), &payment)
                .await
                .is_err()
        );
        let row = repo::find(&pool, checkout).await.unwrap().unwrap();
        assert_eq!(row.order_id, Some(order_id.clone()));
        assert_eq!(row.payment_state, PaymentState::Paid);
        if mismatch {
            assert_eq!(row.completion_state, CompletionState::Operator);
        } else {
            let metadata = payment_metadata(checkout, &payment).unwrap();
            let resumed = Fixture::new(vec![
                reply("draftOrder", draft(&snapshot, Some(&order_id))),
                reply("order", order(&snapshot, &order_id, Some(metadata))),
            ])
            .await;
            assert_eq!(
                apply_verified_payment(&pool, &client(&resumed), &payment)
                    .await
                    .unwrap()
                    .completion_state,
                CompletionState::Complete
            );
        }
        cleanup(&pool, checkout).await;
    }
}
#[tokio::test]
async fn unpaid_facts_are_saved_without_shopify_and_second_payment_is_operator_exception() {
    let pool = pool().await;
    let (checkout, snapshot, attempt, mut payment) = fixture();
    seed(&pool, checkout, &snapshot, &attempt).await;
    let paid = payment.state.clone();
    payment.state = PaymentFact::Expired;
    let f = Fixture::new(vec![]).await;
    assert_eq!(
        apply_verified_payment(&pool, &client(&f), &payment)
            .await
            .unwrap()
            .payment_state,
        PaymentState::Expired
    );
    assert_eq!(f.requests().len(), 0);
    payment.state = paid;
    let order_id = id(
        "Order",
        u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
    );
    let metadata = payment_metadata(checkout, &payment).unwrap();
    let paid_fixture = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, Some(&order_id))),
        reply("order", order(&snapshot, &order_id, Some(metadata))),
    ])
    .await;
    apply_verified_payment(&pool, &client(&paid_fixture), &payment)
        .await
        .unwrap();
    if let PaymentFact::Paid { aliases, .. } = &mut payment.state {
        aliases.last_mut().unwrap().value = "ch_second".into();
    }
    assert_eq!(
        apply_verified_payment(&pool, &client(&f), &payment)
            .await
            .unwrap_err()
            .into_response()
            .status(),
        axum::http::StatusCode::CONFLICT
    );
    assert_eq!(
        repo::find(&pool, checkout).await.unwrap().unwrap().order_id,
        Some(order_id)
    );
    cleanup(&pool, checkout).await;
}

#[tokio::test]
async fn paypal_handoff_and_parallel_replays_share_one_order_and_one_completion() {
    let pool = pool().await;
    let (checkout, snapshot, mut attempt, mut payment) = fixture();
    attempt.provider = Provider::Paypal;
    attempt.reference = format!("PAYPAL{checkout}");
    payment.attempt = attempt.clone();
    payment.state = PaymentFact::Paid {
        money: snapshot.accepted.clone(),
        aliases: vec![
            PaymentAlias {
                kind: AliasKind::PaypalOrder,
                value: attempt.reference.clone(),
            },
            PaymentAlias {
                kind: AliasKind::PaypalCapture,
                value: format!("CAPTURE{checkout}"),
            },
        ],
    };
    seed(&pool, checkout, &snapshot, &attempt).await;
    let order_id = id(
        "Order",
        u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
    );
    let metadata = payment_metadata(checkout, &payment).unwrap();
    let f = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, None)),
        complete_reply(&snapshot, &order_id),
        reply("order", order(&snapshot, &order_id, Some(metadata))),
    ])
    .await;
    let client = client(&f);
    let (first, second) = tokio::join!(
        apply_verified_payment(&pool, &client, &payment),
        apply_verified_payment(&pool, &client, &payment)
    );
    let outcomes = [first, second];
    assert!(outcomes.iter().any(|result| {
        result
            .as_ref()
            .is_ok_and(|outcome| outcome.completion_state == CompletionState::Complete)
    }));
    assert!(outcomes.iter().all(|result| match result {
        Ok(_) => true,
        Err(error) => error.http_status() == 503,
    }));
    let row = repo::find(&pool, checkout).await.unwrap().unwrap();
    assert_eq!(row.order_id, Some(order_id));
    assert_eq!(row.payment_state, PaymentState::Paid);
    assert_eq!(
        f.requests()
            .iter()
            .filter(|r| r.2["operationName"] == "CompleteCheckoutDraft")
            .count(),
        1
    );
    assert_eq!(
        repo::aliases_for_checkout(&pool, checkout)
            .await
            .unwrap()
            .len(),
        2
    );
    cleanup(&pool, checkout).await;
}
#[tokio::test]
async fn draft_and_completion_protocol_failures_preserve_paid_fact_and_never_retry_mutation() {
    let pool = pool().await;
    for stage in ["read", "complete"] {
        let failures = vec![
            Reply::status(503),
            Reply::json(json!({"data":null,"errors":[{"message":"private upstream error"}]})),
            Reply {
                status: 200,
                body: "not json".into(),
                headers: vec![],
            },
            Reply::json(json!({"data":{}})),
            reply(
                "draftOrderComplete",
                json!({"draftOrder":null,"userErrors":[{"field":["id"],"message":"private rejection"}]}),
            ),
        ];
        for fault in failures {
            let (checkout, snapshot, attempt, payment) = fixture();
            seed(&pool, checkout, &snapshot, &attempt).await;
            let replies = if stage == "complete" {
                vec![reply("draftOrder", draft(&snapshot, None)), fault]
            } else {
                vec![fault]
            };
            let f = Fixture::new(replies).await;
            let error = apply_verified_payment(&pool, &client(&f), &payment)
                .await
                .unwrap_err();
            let response = error.into_response();
            assert_eq!(response.status(), 502);
            let body = axum::body::to_bytes(response.into_body(), 10000)
                .await
                .unwrap();
            assert!(!String::from_utf8_lossy(&body).contains("private"));
            let row = repo::find(&pool, checkout).await.unwrap().unwrap();
            assert_eq!(row.payment_state, PaymentState::Paid);
            assert_eq!(row.completion_intent, stage == "complete");
            assert_eq!(f.requests().len(), if stage == "complete" { 2 } else { 1 });
            cleanup(&pool, checkout).await;
        }
    }
}
#[tokio::test]
async fn incomplete_draft_or_order_lines_fail_explicitly_and_keep_saved_identity() {
    let pool = pool().await;
    for draft_incomplete in [true, false] {
        let (checkout, snapshot, attempt, payment) = fixture();
        seed(&pool, checkout, &snapshot, &attempt).await;
        let order_id = id(
            "Order",
            u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
        );
        let mut draft = draft(
            &snapshot,
            if draft_incomplete {
                None
            } else {
                Some(&order_id)
            },
        );
        let mut order = order(&snapshot, &order_id, None);
        if draft_incomplete {
            draft["lineItems"]["pageInfo"]["hasNextPage"] = json!(true);
        } else {
            order["lineItems"]["pageInfo"]["hasNextPage"] = json!(true);
        }
        let f = Fixture::new(vec![reply("draftOrder", draft), reply("order", order)]).await;
        assert_eq!(
            apply_verified_payment(&pool, &client(&f), &payment)
                .await
                .unwrap_err()
                .into_response()
                .status(),
            502
        );
        assert!(
            f.requests()
                .iter()
                .all(|r| r.2["operationName"] != "CompleteCheckoutDraft")
        );
        let row = repo::find(&pool, checkout).await.unwrap().unwrap();
        assert_eq!(row.payment_state, PaymentState::Paid);
        assert_eq!(
            row.order_id,
            if draft_incomplete {
                None
            } else {
                Some(order_id)
            }
        );
        cleanup(&pool, checkout).await;
    }
}
#[tokio::test]
async fn final_order_mismatches_never_write_metadata_or_repeat_completion() {
    let pool = pool().await;
    for field in [
        "email",
        "phone",
        "customAttributes",
        "tags",
        "taxesIncluded",
        "totalPriceSet",
        "totalShippingPriceSet",
        "lineItems",
        "displayFinancialStatus",
        "displayFulfillmentStatus",
    ] {
        let (checkout, snapshot, attempt, payment) = fixture();
        seed(&pool, checkout, &snapshot, &attempt).await;
        let order_id = id(
            "Order",
            u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
        );
        let mut inspected = order(&snapshot, &order_id, None);
        match field {
            "email" | "phone" => inspected[field] = json!("mismatch"),
            "customAttributes" | "tags" => inspected[field] = json!([]),
            "taxesIncluded" => inspected[field] = json!(false),
            "totalPriceSet" => inspected[field] = bag("246.91"),
            "totalShippingPriceSet" => inspected[field] = bag("1.00"),
            "lineItems" => inspected[field]["nodes"][0]["quantity"] = json!(1),
            "displayFinancialStatus" => inspected[field] = json!("PENDING"),
            "displayFulfillmentStatus" => inspected[field] = json!("FULFILLED"),
            _ => unreachable!(),
        }
        let f = Fixture::new(vec![
            reply("draftOrder", draft(&snapshot, Some(&order_id))),
            reply("order", inspected),
        ])
        .await;
        assert_eq!(
            apply_verified_payment(&pool, &client(&f), &payment)
                .await
                .unwrap_err()
                .into_response()
                .status(),
            409,
            "{field}"
        );
        assert_eq!(f.requests().len(), 2);
        let row = repo::find(&pool, checkout).await.unwrap().unwrap();
        assert_eq!(row.order_id, Some(order_id));
        assert_eq!(row.completion_state, CompletionState::Operator);
        cleanup(&pool, checkout).await;
    }
}
#[tokio::test]
async fn second_payment_operator_record_retains_original_order_and_new_payment_evidence() {
    let pool = pool().await;
    let (checkout, snapshot, attempt, mut payment) = fixture();
    seed(&pool, checkout, &snapshot, &attempt).await;
    let order_id = id(
        "Order",
        u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
    );
    let metadata = payment_metadata(checkout, &payment).unwrap();
    let f = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, Some(&order_id))),
        reply("order", order(&snapshot, &order_id, Some(metadata))),
    ])
    .await;
    apply_verified_payment(&pool, &client(&f), &payment)
        .await
        .unwrap();
    if let PaymentFact::Paid { aliases, .. } = &mut payment.state {
        aliases.last_mut().unwrap().value = "ch_operator_evidence".into();
    }
    assert!(
        apply_verified_payment(&pool, &client(&f), &payment)
            .await
            .is_err()
    );
    let evidence: String =
        sqlx::query_scalar("SELECT operator_payment::text FROM checkout_completions WHERE id=$1")
            .bind(checkout)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(evidence.contains("ch_operator_evidence"));
    let row = repo::find(&pool, checkout).await.unwrap().unwrap();
    assert_eq!(row.order_id, Some(order_id));
    let Some(PaymentFact::Paid { aliases, .. }) = row.trusted_payment else {
        panic!("paid")
    };
    assert!(
        aliases
            .iter()
            .all(|alias| alias.value != "ch_operator_evidence")
    );
    cleanup(&pool, checkout).await;
}
#[tokio::test]
async fn refused_connection_and_timeout_leave_paid_recoverable_without_mutation() {
    use tokio::io::AsyncReadExt;
    let pool = pool().await;
    for timeout in [false, true] {
        let (checkout, snapshot, attempt, payment) = fixture();
        seed(&pool, checkout, &snapshot, &attempt).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = if timeout {
            Some(tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = [0; 4096];
                let _ = socket.read(&mut bytes).await;
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }))
        } else {
            drop(listener);
            None
        };
        let client = ShopifyClient::new(&crate::config::ShopifyConfig {
            admin_api_url: format!("http://{addr}/graphql.json"),
            access_token: Some("fixture".into()),
            timeout_secs: 1,
        })
        .unwrap();
        let response = apply_verified_payment(&pool, &client, &payment)
            .await
            .unwrap_err()
            .into_response();
        assert_eq!(response.status(), if timeout { 504 } else { 502 });
        let row = repo::find(&pool, checkout).await.unwrap().unwrap();
        assert_eq!(row.payment_state, PaymentState::Paid);
        assert!(!row.completion_intent);
        if let Some(task) = task {
            task.abort();
        }
        cleanup(&pool, checkout).await;
    }
}

#[tokio::test]
async fn local_order_save_failure_reconciles_without_another_completion() {
    let pool = pool().await;
    let (checkout, snapshot, attempt, payment) = fixture();
    seed(&pool, checkout, &snapshot, &attempt).await;
    let name = format!("owned_checkout_save_{}", checkout.simple());
    let sql = format!(
        "CREATE FUNCTION {name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.id='{checkout}'::uuid AND NEW.order_id IS NOT NULL AND OLD.order_id IS NULL THEN RAISE EXCEPTION 'owned order save fault'; END IF; RETURN NEW; END; $$; CREATE TRIGGER {name} BEFORE UPDATE ON checkout_completions FOR EACH ROW EXECUTE FUNCTION {name}();"
    );
    sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
        .execute(&pool)
        .await
        .unwrap();
    let order_id = id(
        "Order",
        u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
    );
    let f = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, None)),
        complete_reply(&snapshot, &order_id),
    ])
    .await;
    let result = apply_verified_payment(&pool, &client(&f), &payment).await;
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP TRIGGER {name} ON checkout_completions; DROP FUNCTION {name}();"
    )))
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().into_response().status(), 500);
    let row = repo::find(&pool, checkout).await.unwrap().unwrap();
    assert_eq!(row.payment_state, PaymentState::Paid);
    assert!(row.completion_intent);
    assert!(row.order_id.is_none());
    let metadata = payment_metadata(checkout, &payment).unwrap();
    let resumed = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, Some(&order_id))),
        reply("order", order(&snapshot, &order_id, Some(metadata))),
    ])
    .await;
    assert_eq!(
        apply_verified_payment(&pool, &client(&resumed), &payment)
            .await
            .unwrap()
            .completion_state,
        CompletionState::Complete
    );
    assert!(
        resumed
            .requests()
            .iter()
            .all(|r| r.2["operationName"] != "CompleteCheckoutDraft")
    );
    cleanup(&pool, checkout).await;
}
#[tokio::test]
async fn metadata_visibility_failure_remains_resumable_against_saved_order() {
    let pool = pool().await;
    let (checkout, snapshot, attempt, payment) = fixture();
    seed(&pool, checkout, &snapshot, &attempt).await;
    let order_id = id(
        "Order",
        u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
    );
    let f = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, Some(&order_id))),
        reply("order", order(&snapshot, &order_id, None)),
        metadata_reply(),
        reply("order", order(&snapshot, &order_id, None)),
    ])
    .await;
    assert_eq!(
        apply_verified_payment(&pool, &client(&f), &payment)
            .await
            .unwrap_err()
            .into_response()
            .status(),
        502
    );
    let row = repo::find(&pool, checkout).await.unwrap().unwrap();
    assert_eq!(row.order_id, Some(order_id.clone()));
    assert_eq!(row.completion_state, CompletionState::OrderSaved);
    let metadata = payment_metadata(checkout, &payment).unwrap();
    let resumed = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, Some(&order_id))),
        reply("order", order(&snapshot, &order_id, Some(metadata))),
    ])
    .await;
    assert_eq!(
        apply_verified_payment(&pool, &client(&resumed), &payment)
            .await
            .unwrap()
            .completion_state,
        CompletionState::Complete
    );
    cleanup(&pool, checkout).await;
}

#[tokio::test]
async fn externally_claimed_verification_uses_same_fence_and_rejects_stale_owner() {
    let pool = pool().await;
    let (checkout, snapshot, attempt, payment) = fixture();
    seed(&pool, checkout, &snapshot, &attempt).await;
    let token = Uuid::new_v4();
    repo::claim(
        &mut pool.acquire().await.unwrap(),
        checkout,
        token,
        Utc::now() + chrono::Duration::seconds(120),
    )
    .await
    .unwrap()
    .unwrap();
    let f = Fixture::new(vec![]).await;
    assert_eq!(
        apply_claimed_verified_payment(&pool, &client(&f), &payment, Uuid::new_v4())
            .await
            .unwrap_err()
            .into_response()
            .status(),
        409
    );
    assert_eq!(f.requests().len(), 0);
    let order_id = id(
        "Order",
        u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
    );
    let metadata = payment_metadata(checkout, &payment).unwrap();
    let f = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, Some(&order_id))),
        reply("order", order(&snapshot, &order_id, Some(metadata))),
    ])
    .await;
    assert_eq!(
        apply_claimed_verified_payment(&pool, &client(&f), &payment, token)
            .await
            .unwrap()
            .completion_state,
        CompletionState::Complete
    );
    cleanup(&pool, checkout).await;
}

#[tokio::test]
async fn later_alias_for_same_payment_is_preserved_and_updates_only_owned_metadata() {
    let pool = pool().await;
    let (checkout, snapshot, attempt, payment) = fixture();
    seed(&pool, checkout, &snapshot, &attempt).await;
    let mut initial = payment.clone();
    if let PaymentFact::Paid { aliases, .. } = &mut initial.state {
        aliases.retain(|alias| alias.kind != AliasKind::StripePaymentIntent);
    }
    let order_id = id(
        "Order",
        u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
    );
    let initial_metadata = payment_metadata(checkout, &initial).unwrap();
    let f = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, Some(&order_id))),
        reply(
            "order",
            order(&snapshot, &order_id, Some(initial_metadata.clone())),
        ),
    ])
    .await;
    apply_verified_payment(&pool, &client(&f), &initial)
        .await
        .unwrap();
    let metadata = payment_metadata(checkout, &payment).unwrap();
    let f = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, Some(&order_id))),
        reply("order", order(&snapshot, &order_id, Some(initial_metadata))),
        metadata_reply(),
        reply("order", order(&snapshot, &order_id, Some(metadata))),
    ])
    .await;
    assert_eq!(
        apply_verified_payment(&pool, &client(&f), &payment)
            .await
            .unwrap()
            .completion_state,
        CompletionState::Complete
    );
    assert_eq!(
        repo::aliases_for_checkout(&pool, checkout)
            .await
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        f.requests()[2].2["variables"]["metafields"][0]["compareDigest"],
        "digest"
    );
    assert!(
        f.requests()
            .iter()
            .all(|r| r.2["operationName"] != "CompleteCheckoutDraft")
    );
    cleanup(&pool, checkout).await;
}

#[tokio::test]
async fn transient_payment_fact_sql_failure_remains_retryable() {
    let pool = pool().await;
    for alias_fault in [true, false] {
        let (checkout, snapshot, attempt, payment) = fixture();
        seed(&pool, checkout, &snapshot, &attempt).await;
        let name = format!("owned_payment_fault_{}", checkout.simple());
        let (table, condition, event) = if alias_fault {
            (
                "checkout_payment_aliases",
                format!("NEW.checkout_id='{checkout}'::uuid"),
                "INSERT",
            )
        } else {
            (
                "checkout_completions",
                format!(
                    "NEW.id='{checkout}'::uuid AND NEW.trusted_payment IS NOT NULL AND NEW.completion_state<>'OPERATOR'"
                ),
                "UPDATE",
            )
        };
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE FUNCTION {name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF {condition} THEN RAISE EXCEPTION 'owned transient fact fault'; END IF; RETURN NEW; END; $$; CREATE TRIGGER {name} BEFORE {event} ON {table} FOR EACH ROW EXECUTE FUNCTION {name}();"
        )))
        .execute(&pool).await.unwrap();
        let f = Fixture::new(vec![]).await;
        let result = apply_verified_payment(&pool, &client(&f), &payment).await;
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "DROP TRIGGER {name} ON {table}; DROP FUNCTION {name}();"
        )))
        .execute(&pool)
        .await
        .unwrap();
        let row = repo::find(&pool, checkout).await.unwrap().unwrap();
        let status = result.unwrap_err().http_status();
        let order_id = id(
            "Order",
            u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
        );
        let metadata = payment_metadata(checkout, &payment).unwrap();
        let resumed = Fixture::new(vec![
            reply("draftOrder", draft(&snapshot, Some(&order_id))),
            reply("order", order(&snapshot, &order_id, Some(metadata))),
        ])
        .await;
        let outcome = apply_verified_payment(&pool, &client(&resumed), &payment).await;
        cleanup(&pool, checkout).await;
        assert_eq!(status, 500);
        assert_ne!(row.completion_state, CompletionState::Operator);
        assert!(row.trusted_payment.is_none());
        assert!(row.lease_token.is_none());
        assert_eq!(f.requests().len(), 0);
        assert_eq!(outcome.unwrap().completion_state, CompletionState::Complete);
    }
}

#[tokio::test]
async fn busy_complete_checkout_rejects_unprocessed_payment_and_replay_retains_evidence() {
    let pool = pool().await;
    let (checkout, snapshot, attempt, mut payment) = fixture();
    seed(&pool, checkout, &snapshot, &attempt).await;
    let order_id = id(
        "Order",
        u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
    );
    let metadata = payment_metadata(checkout, &payment).unwrap();
    let f = Fixture::new(vec![
        reply("draftOrder", draft(&snapshot, Some(&order_id))),
        reply("order", order(&snapshot, &order_id, Some(metadata))),
    ])
    .await;
    apply_verified_payment(&pool, &client(&f), &payment)
        .await
        .unwrap();
    let token = Uuid::new_v4();
    repo::claim(
        &mut pool.acquire().await.unwrap(),
        checkout,
        token,
        Utc::now() + chrono::Duration::seconds(120),
    )
    .await
    .unwrap()
    .unwrap();
    let same = apply_verified_payment(&pool, &client(&f), &payment).await;
    if let PaymentFact::Paid { aliases, .. } = &mut payment.state {
        aliases.last_mut().unwrap().value = format!("ch_second_{checkout}");
    }
    let second = apply_verified_payment(&pool, &client(&f), &payment).await;
    repo::progress(
        &pool,
        checkout,
        token,
        CompletionState::Complete,
        None,
        false,
        None,
        true,
    )
    .await
    .unwrap();
    let replay = apply_verified_payment(&pool, &client(&f), &payment).await;
    let row = repo::find(&pool, checkout).await.unwrap().unwrap();
    let evidence: String =
        sqlx::query_scalar("SELECT operator_payment::text FROM checkout_completions WHERE id=$1")
            .bind(checkout)
            .fetch_one(&pool)
            .await
            .unwrap();
    cleanup(&pool, checkout).await;
    assert_eq!(same.unwrap_err().http_status(), 503);
    assert_eq!(second.unwrap_err().http_status(), 503);
    assert_eq!(replay.unwrap_err().http_status(), 409);
    assert_eq!(row.completion_state, CompletionState::Operator);
    assert_eq!(row.order_id, Some(order_id));
    assert!(evidence.contains(&format!("ch_second_{checkout}")));
    assert_eq!(f.requests().len(), 2);
}

#[tokio::test]
async fn omitted_nullable_shopify_safety_fields_reject_incomplete_reads() {
    let pool = pool().await;
    for field in [
        "paymentTerms",
        "order",
        "reserveInventoryUntil",
        "phone",
        "cancelledAt",
        "metafield",
    ] {
        let (checkout, mut snapshot, attempt, payment) = fixture();
        if field == "phone" {
            snapshot.phone = None;
        }
        seed(&pool, checkout, &snapshot, &attempt).await;
        let order_id = id(
            "Order",
            u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
        );
        let mut draft_value = draft(&snapshot, None);
        let metadata = payment_metadata(checkout, &payment).unwrap();
        let mut order_value = order(&snapshot, &order_id, Some(metadata.clone()));
        let replies =
            if ["paymentTerms", "order", "reserveInventoryUntil", "phone"].contains(&field) {
                draft_value.as_object_mut().unwrap().remove(field);
                vec![
                    reply("draftOrder", draft_value),
                    complete_reply(&snapshot, &order_id),
                    reply("order", order_value),
                ]
            } else {
                order_value.as_object_mut().unwrap().remove(field);
                vec![
                    reply("draftOrder", draft(&snapshot, Some(&order_id))),
                    reply("order", order_value),
                    metadata_reply(),
                    reply("order", order(&snapshot, &order_id, Some(metadata))),
                ]
            };
        let f = Fixture::new(replies).await;
        let result = apply_verified_payment(&pool, &client(&f), &payment).await;
        let row = repo::find(&pool, checkout).await.unwrap().unwrap();
        cleanup(&pool, checkout).await;
        assert_eq!(result.unwrap_err().http_status(), 502, "{field}");
        assert_ne!(row.completion_state, CompletionState::Complete, "{field}");
        assert!(
            f.requests()
                .iter()
                .all(|request| request.2["operationName"] != "CompleteCheckoutDraft"),
            "{field}"
        );
    }
}

#[tokio::test]
async fn malformed_shopify_money_remains_recoverable_and_same_payment_replay_completes() {
    let pool = pool().await;
    for amount in ["invalid", "9223372036854775808.00", "246.901"] {
        let (checkout, snapshot, attempt, payment) = fixture();
        seed(&pool, checkout, &snapshot, &attempt).await;
        let order_id = id(
            "Order",
            u64::from_be_bytes(checkout.as_bytes()[8..].try_into().unwrap()),
        );
        let mut invalid = draft(&snapshot, None);
        invalid["totalPriceSet"]["presentmentMoney"]["amount"] = json!(amount);
        let broken = Fixture::new(vec![reply("draftOrder", invalid)]).await;
        let result = apply_verified_payment(&pool, &client(&broken), &payment).await;
        let row = repo::find(&pool, checkout).await.unwrap().unwrap();
        if result
            .as_ref()
            .is_err_and(|error| error.http_status() != 502)
        {
            cleanup(&pool, checkout).await;
        }
        assert_eq!(result.unwrap_err().http_status(), 502, "{amount}");
        assert_eq!(row.payment_state, PaymentState::Paid);
        assert_ne!(row.completion_state, CompletionState::Operator);
        assert!(!row.completion_intent);
        assert!(row.order_id.is_none());
        assert_eq!(broken.requests().len(), 1);
        let metadata = payment_metadata(checkout, &payment).unwrap();
        let recovered = Fixture::new(vec![
            reply("draftOrder", draft(&snapshot, None)),
            complete_reply(&snapshot, &order_id),
            reply("order", order(&snapshot, &order_id, None)),
            metadata_reply(),
            reply("order", order(&snapshot, &order_id, Some(metadata))),
        ])
        .await;
        let result = apply_verified_payment(&pool, &client(&recovered), &payment).await;
        cleanup(&pool, checkout).await;
        let result = result.unwrap();
        assert_eq!(result.completion_state, CompletionState::Complete);
        assert_eq!(result.order_id, Some(order_id));
        assert_eq!(
            recovered
                .requests()
                .iter()
                .filter(|request| request.2["operationName"] == "CompleteCheckoutDraft")
                .count(),
            1
        );
    }
}

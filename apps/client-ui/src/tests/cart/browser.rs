#![cfg(not(target_arch = "wasm32"))]
use super::*;
use crate::pages::tests::{CHECKOUT, TestDocument};
use std::rc::Rc;

async fn scoped<T>(document: Rc<TestDocument>, future: impl std::future::Future<Output = T>) -> T {
    let dom = VirtualDom::new(|| rsx! {}).with_root_context(document as Rc<dyn document::Document>);
    let mut future = Box::pin(future);
    std::future::poll_fn(|cx| dom.in_scope(ScopeId::ROOT, || future.as_mut().poll(cx))).await
}
#[tokio::test]
async fn bridge_restores_only_valid_owned_carts_and_reports_corruption_storage_and_uuid_failures() {
    let mut cart = Cart::new("event".into(), CHECKOUT.into());
    cart.add(&crate::tests::variant(), 2)
        .expect("add owned fixture variant");
    let raw = serde_json::to_string(&cart).expect("serialize owned cart");
    let document = TestDocument::new(vec![
        serde_json::json!({"raw":raw,"checkout_id":CHECKOUT,"blocked":false}),
    ]);
    let restored = scoped(document.clone(), restore("event"))
        .await
        .expect("restore controlled cart");
    assert_eq!(restored.cart, cart);
    assert!(restored.notice.is_none());
    assert!(document.scripts.borrow()[0].contains("enzumeii.cart.v1.event"));
    for (raw, blocked, notice) in [
        (Some("{"), false, "could not be restored"),
        (None, true, "unavailable"),
        (None, false, ""),
    ] {
        let document = TestDocument::new(vec![
            serde_json::json!({"raw":raw,"checkout_id":CHECKOUT,"blocked":blocked}),
        ]);
        let restored = scoped(document, restore("event"))
            .await
            .expect("restore controlled cart");
        assert_eq!(restored.cart.items, Vec::new());
        if notice.is_empty() {
            assert!(restored.notice.is_none());
        } else {
            assert!(restored.notice.expect("recovery notice").contains(notice));
        }
    }
    let document = TestDocument::new(vec![
        serde_json::json!({"raw":null,"checkout_id":"not-a-uuid","blocked":false}),
    ]);
    assert!(
        scoped(document, restore("event"))
            .await
            .err()
            .expect("restore must reject invalid browser response")
            .contains("identifier")
    );
    assert!(
        scoped(TestDocument::new(vec![]), restore("event"))
            .await
            .err()
            .expect("restore must reject invalid browser response")
            .contains("initialize")
    );
}
#[tokio::test]
async fn bridge_saves_json_as_string_and_surfaces_failed_or_missing_storage_acknowledgement() {
    let cart = Cart::new("event\"quoted".into(), CHECKOUT.into());
    let document = TestDocument::new(vec![serde_json::json!(true)]);
    scoped(document.clone(), save(&cart))
        .await
        .expect("browser acknowledged cart save");
    let script = document.scripts.borrow()[0].clone();
    assert!(script.contains(
        &serde_json::to_string(&storage_key(&cart.convention_id)).expect("encode storage key")
    ));
    assert!(
        script.contains(
            &serde_json::to_string(&serde_json::to_string(&cart).expect("serialize owned cart"))
                .expect("encode cart JSON as script string")
        )
    );
    for replies in [vec![serde_json::json!(false)], vec![]] {
        assert!(
            scoped(TestDocument::new(replies), save(&cart))
                .await
                .expect_err("controlled browser failure")
                .contains("Changes remain in this tab")
        );
    }
    assert_eq!(
        scoped(
            TestDocument::new(vec![serde_json::json!(["2099-01-01", "TH"])]),
            visitor()
        )
        .await
        .expect("controlled visitor date and country"),
        ("2099-01-01".into(), Some("TH".into()))
    );
    assert!(
        scoped(TestDocument::new(vec![]), visitor())
            .await
            .expect_err("controlled browser failure")
            .contains("local date")
    );
}

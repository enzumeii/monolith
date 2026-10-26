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
    let document = TestDocument::new(vec![
        serde_json::json!({"__eval_messages":[{"raw":null},true]}),
    ]);
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
    for replies in [
        vec![serde_json::json!(false)],
        vec![serde_json::json!({"__eval_messages":[{"raw":null},false]})],
        vec![],
    ] {
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
            visitor(&crate::config::tests::settings().visitor)
        )
        .await
        .expect("controlled visitor date and country"),
        ("2099-01-01".into(), Some("TH".into()))
    );
    assert!(
        scoped(
            TestDocument::new(vec![]),
            visitor(&crate::config::tests::settings().visitor)
        )
        .await
        .expect_err("controlled browser failure")
        .contains("local date")
    );
}

#[tokio::test]
async fn browser_failures_offer_specific_guidance_without_reload_requests() {
    let error = scoped(
        TestDocument::new(vec![]),
        visitor(&crate::config::tests::settings().visitor),
    )
    .await
    .expect_err("date failure");
    assert!(error.contains("JavaScript"));
    assert!(!error.contains("reload"));
    let error = scoped(TestDocument::new(vec![]), restore("event"))
        .await
        .err()
        .expect("cart failure");
    assert!(error.contains("browser storage"));
    assert!(!error.contains("reload"));
    let document = TestDocument::new(vec![
        serde_json::json!({"raw":null,"checkout_id":"invalid","blocked":false}),
    ]);
    let error = scoped(document, restore("event"))
        .await
        .err()
        .expect("identifier failure");
    assert!(error.contains("secure browser"));
    assert!(!error.contains("reload"));
}

#[tokio::test]
async fn visitor_bridge_injects_only_typed_lookup_settings_into_one_browser_evaluation() {
    let settings = crate::config::tests::settings().visitor;
    let document = TestDocument::new(vec![serde_json::json!(["2099-01-01", "TH"])]);
    let visitor = scoped(document.clone(), visitor(&settings))
        .await
        .expect("controlled visitor lookup");
    assert_eq!(visitor.1.as_deref(), Some("TH"));
    let scripts = document.scripts.borrow();
    assert_eq!(scripts.len(), 1);
    assert!(
        scripts[0]
            .contains(&serde_json::to_string(&settings.country_lookup_url).expect("JSON URL"))
    );
    assert!(scripts[0].contains(&settings.timeout_ms.to_string()));
    assert!(!scripts[0].contains("__LOOKUP_URL__"));
    assert!(!scripts[0].contains("__TIMEOUT_MS__"));
}

#[tokio::test]
async fn stale_cart_save_returns_current_owned_cart_without_persisting_old_identity() {
    let mut old = Cart::new("event".into(), CHECKOUT.into());
    old.add(&crate::tests::variant(), 1).expect("old selection");
    let mut current = old.clone();
    current.checkout_id = "b65bbebd-f990-4599-bc72-daa653abc900".into();
    current.items[0].quantity = 3;
    let document = TestDocument::new(vec![serde_json::json!({"__eval_messages":[
        {"raw":serde_json::to_string(&current).expect("current cart JSON")},true
    ]})]);
    assert_eq!(
        scoped(document.clone(), save(&old))
            .await
            .expect("preserve current cart"),
        current
    );
    assert_eq!(*document.sent.borrow(), vec![serde_json::json!(false)]);
    assert!(document.scripts.borrow()[0].contains("enzumeii.cart.v1.${convention}"));
}

#[tokio::test]
async fn malformed_cart_is_replaced_and_cancelled_save_releases_without_writing() {
    use futures::FutureExt;
    let cart = Cart::new("event".into(), CHECKOUT.into());
    let document = TestDocument::new(vec![
        serde_json::json!({"__eval_messages":[{"raw":"{"},true]}),
    ]);
    assert_eq!(
        scoped(document.clone(), save(&cart))
            .await
            .expect("replace malformed cart"),
        cart
    );
    assert_eq!(*document.sent.borrow(), vec![serde_json::json!(true)]);
    let document = TestDocument::new(vec![serde_json::json!({"__eval_pending":true})]);
    assert!(
        scoped(document.clone(), save(&cart))
            .now_or_never()
            .is_none()
    );
    assert_eq!(*document.sent.borrow(), vec![serde_json::json!(false)]);
}

use super::*;
use crate::pages::tests::TestDocument;
use dioxus::document::Document;
use futures::FutureExt;

fn attempt() -> Attempt {
    Attempt {
        version: 1,
        operation_id: "a2913eb5-722d-41a3-a521-632e7fa8cc9a".into(),
        secret: "ab".repeat(32),
        slug: "fair-a".into(),
        provider: domain::dto::checkout::CheckoutProvider::Stripe,
        snapshot: Cart::new(
            "event".into(),
            "f79b7e64-8e36-4aca-9e21-ecb7cb513700".into(),
        ),
        state: "submitted".into(),
        currency: "USD".into(),
        total: "24.60".into(),
    }
}

#[test]
fn cancelling_while_waiting_for_ownership_releases_the_web_lock_without_retiring_attempt() {
    let document = TestDocument::new(vec![json!({"__eval_pending":true})]);
    let dom = VirtualDom::new(|| rsx! {})
        .with_root_context(document.clone() as std::rc::Rc<dyn Document>);
    let attempt = attempt();
    let mut pending = Box::pin(recover(&attempt));
    assert!(
        dom.in_scope(ScopeId::ROOT, || pending.as_mut().now_or_never())
            .is_none()
    );
    drop(pending);
    assert_eq!(*document.sent.borrow(), vec![json!({"status":null})]);
}

#[test]
fn cancelling_after_ownership_during_inflight_work_preserves_ambiguity_and_releases_once() {
    let attempt = attempt();
    let document = TestDocument::new(vec![
        json!({"ok":true,"value":{"attempt":attempt,"payload":null,"fresh":false}}),
    ]);
    let dom = VirtualDom::new(|| rsx! {})
        .with_root_context(document.clone() as std::rc::Rc<dyn Document>);
    let mut pending = Box::pin(async {
        let (ownership, _) = recover(&attempt).await.expect("ownership acquired");
        std::future::pending::<()>().await;
        drop(ownership);
    });
    assert!(
        dom.in_scope(ScopeId::ROOT, || pending.as_mut().now_or_never())
            .is_none()
    );
    assert!(document.sent.borrow().is_empty());
    drop(pending);
    assert_eq!(*document.sent.borrow(), vec![json!({"status":null})]);
}

#[test]
fn unmounting_checkout_resource_releases_browser_ownership_before_and_after_acquisition() {
    fn app() -> Element {
        let _work = use_resource(|| async {
            let attempt = attempt();
            let (ownership, _) = recover(&attempt).await.expect("ownership acquired");
            std::future::pending::<()>().await;
            drop(ownership);
        });
        rsx! { p { "Checkout in flight" } }
    }
    for pending in [true, false] {
        let reply = if pending {
            json!({"__eval_pending":true})
        } else {
            json!({"ok":true,"value":{"attempt":attempt(),"payload":null,"fresh":false}})
        };
        let document = TestDocument::new(vec![reply]);
        let mut dom =
            VirtualDom::new(app).with_root_context(document.clone() as std::rc::Rc<dyn Document>);
        dom.rebuild_in_place();
        for _ in 0..5 {
            let _ = dom.wait_for_work().now_or_never();
            dom.render_immediate_to_vec();
        }
        assert_eq!(document.scripts.borrow().len(), 1);
        assert!(document.sent.borrow().is_empty());
        drop(dom);
        assert_eq!(*document.sent.borrow(), vec![json!({"status":null})]);
    }
}

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

fn payment_response(url: &str) -> CheckoutResponse {
    use domain::dto::checkout::{CheckoutMoney, CheckoutNextAction, CheckoutStatus};
    CheckoutResponse {
        checkout_id: attempt().operation_id,
        status: CheckoutStatus::RequiresAction,
        money: CheckoutMoney {
            minor: 2460,
            currency: "USD".into(),
        },
        reservation_expires_at: None,
        next_action: Some(CheckoutNextAction::Redirect { url: url.into() }),
        order: None,
    }
}

#[test]
fn payment_window_reservation_runs_before_polling_and_drop_closes_unused_context() {
    let document = TestDocument::new(vec![json!({"__eval_pending":true})]);
    let dom = VirtualDom::new(|| rsx! {})
        .with_root_context(document.clone() as std::rc::Rc<dyn Document>);
    let window = dom.in_scope(ScopeId::ROOT, PaymentWindow::reserve);
    assert_eq!(document.scripts.borrow().len(), 1);
    assert!(document.scripts.borrow()[0].contains("window.open('about:blank', '_blank')"));
    drop(window);
    assert_eq!(*document.sent.borrow(), vec![json!({"action":"abort"})]);
}

#[test]
fn payment_window_handoff_validates_selected_provider_and_releases_without_closing() {
    let document = TestDocument::new(vec![json!({"__eval_messages":[
        {"ok":true,"value":null}, {"ok":true,"value":null}
    ]})]);
    let dom = VirtualDom::new(|| rsx! {})
        .with_root_context(document.clone() as std::rc::Rc<dyn Document>);
    let response = payment_response("https://checkout.stripe.com/c/pay/test");
    let result = dom.in_scope(ScopeId::ROOT, || {
        async {
            let mut window = PaymentWindow::reserve();
            window
                .ready()
                .await
                .expect("reservation confirmed before creation");
            window
                .handoff(&response, CheckoutProvider::Stripe, None)
                .await
        }
        .now_or_never()
    });
    assert_eq!(result, Some(Ok(true)));
    assert_eq!(
        *document.sent.borrow(),
        vec![
            json!({"action":"open","url":"https://checkout.stripe.com/c/pay/test","checkout_id":response.checkout_id})
        ]
    );
}

#[test]
fn unsafe_or_wrong_provider_destination_aborts_reservation_without_handoff() {
    for url in [
        "https://evil.example/pay",
        "https://www.paypal.com/checkoutnow?token=test",
        "javascript:alert(1)",
    ] {
        let document = TestDocument::new(vec![json!({"ok":true,"value":null})]);
        let dom = VirtualDom::new(|| rsx! {})
            .with_root_context(document.clone() as std::rc::Rc<dyn Document>);
        let response = payment_response(url);
        let result = dom.in_scope(ScopeId::ROOT, || {
            PaymentWindow::reserve()
                .handoff(&response, CheckoutProvider::Stripe, None)
                .now_or_never()
        });
        assert!(matches!(result, Some(Err(_))));
        assert_eq!(*document.sent.borrow(), vec![json!({"action":"abort"})]);
    }
}

#[test]
fn blocked_window_is_not_ready_and_non_redirect_results_close_the_unused_context() {
    let document = TestDocument::new(vec![
        json!({"ok":false,"error":"Allow pop-ups for this shop."}),
    ]);
    let dom = VirtualDom::new(|| rsx! {})
        .with_root_context(document.clone() as std::rc::Rc<dyn Document>);
    let result = dom.in_scope(ScopeId::ROOT, || {
        async {
            let mut window = PaymentWindow::reserve();
            window.ready().await
        }
        .now_or_never()
    });
    assert!(matches!(result, Some(Err(message)) if message.contains("Allow pop-ups")));
    assert_eq!(*document.sent.borrow(), vec![json!({"action":"abort"})]);

    let document = TestDocument::new(vec![]);
    let dom = VirtualDom::new(|| rsx! {})
        .with_root_context(document.clone() as std::rc::Rc<dyn Document>);
    let mut response = payment_response("https://checkout.stripe.com/c/pay/test");
    response.status = domain::dto::checkout::CheckoutStatus::Processing;
    let result = dom.in_scope(ScopeId::ROOT, || {
        PaymentWindow::reserve()
            .handoff(&response, CheckoutProvider::Stripe, None)
            .now_or_never()
    });
    assert_eq!(result, Some(Ok(false)));
    assert_eq!(*document.sent.borrow(), vec![json!({"action":"abort"})]);
}

#[test]
fn abandonment_and_ignore_status_pass_only_the_attempt_identity() {
    let cart = attempt().snapshot;
    let document = TestDocument::new(vec![
        json!({"ok":true,"value":true}),
        json!({"ok":true,"value":cart}),
    ]);
    let dom = VirtualDom::new(|| rsx! {})
        .with_root_context(document.clone() as std::rc::Rc<dyn Document>);
    let attempt = attempt();
    dom.in_scope(ScopeId::ROOT, || {
        async {
            assert!(ignored(&attempt).await.expect("ignore decision read"));
            assert_eq!(abandon(&attempt).await.expect("cart fork"), cart);
        }
        .now_or_never()
    })
    .expect("ready actions");
    for (script, action) in document.scripts.borrow().iter().zip(["ignored", "abandon"]) {
        assert!(script.contains(&format!("\"action\":\"{action}\"")));
        assert!(script.contains(&attempt.operation_id));
        assert!(!script.contains(&attempt.secret));
    }
    assert!(document.sent.borrow().is_empty());
}

#[test]
fn checking_session_waits_then_owns_and_releases_once_when_disposed() {
    let document = TestDocument::new(vec![json!({"__eval_messages":[
        {"ok":true,"value":"passive"}, {"ok":true,"value":"owner"}
    ]})]);
    let dom = VirtualDom::new(|| rsx! {})
        .with_root_context(document.clone() as std::rc::Rc<dyn Document>);
    dom.in_scope(ScopeId::ROOT, || {
        async {
            let mut session = CheckingSession::new(&attempt()).expect("checking session");
            assert_eq!(
                session.next().await.expect("passive"),
                CheckingState::Passive
            );
            assert_eq!(session.next().await.expect("owner"), CheckingState::Owner);
            drop(session);
        }
        .now_or_never()
    })
    .expect("ready events");
    assert_eq!(*document.sent.borrow(), vec![json!({"action":"release"})]);
}

#[test]
fn checking_session_disposal_aborts_pending_acquisition_and_ignore_is_already_released() {
    for ignored in [false, true] {
        let reply = if ignored {
            json!({"ok":true,"value":"ignored"})
        } else {
            json!({"__eval_pending":true})
        };
        let document = TestDocument::new(vec![reply]);
        let dom = VirtualDom::new(|| rsx! {})
            .with_root_context(document.clone() as std::rc::Rc<dyn Document>);
        let mut future = Box::pin(async {
            let mut session = CheckingSession::new(&attempt()).expect("session");
            session.next().await
        });
        let result = dom.in_scope(ScopeId::ROOT, || future.as_mut().now_or_never());
        if ignored {
            assert_eq!(result, Some(Ok(CheckingState::Ignored)));
        } else {
            assert!(result.is_none());
        }
        drop(future);
        let expected = if ignored {
            Vec::new()
        } else {
            vec![json!({"action":"release"})]
        };
        assert_eq!(*document.sent.borrow(), expected);
    }
}

#[test]
fn malformed_checking_event_still_releases_ownership() {
    let document = TestDocument::new(vec![json!({"ok":true,"value":"unexpected"})]);
    let dom = VirtualDom::new(|| rsx! {})
        .with_root_context(document.clone() as std::rc::Rc<dyn Document>);
    dom.in_scope(ScopeId::ROOT, || {
        async {
            let mut session = CheckingSession::new(&attempt()).expect("session");
            assert!(session.next().await.is_err());
        }
        .now_or_never()
    })
    .expect("ready malformed event");
    assert_eq!(*document.sent.borrow(), vec![json!({"action":"release"})]);
}

#[test]
fn dropping_an_unpolled_checking_session_releases_its_pending_acquisition() {
    let document = TestDocument::new(vec![json!({"__eval_pending":true})]);
    let dom = VirtualDom::new(|| rsx! {})
        .with_root_context(document.clone() as std::rc::Rc<dyn Document>);
    let session = dom
        .in_scope(ScopeId::ROOT, || CheckingSession::new(&attempt()))
        .expect("checking session");
    drop(session);
    assert_eq!(*document.sent.borrow(), vec![json!({"action":"release"})]);
}

#[test]
fn empty_report_disables_save_before_opening_a_picker() {
    use dioxus::{
        dioxus_core::{AttributeValue, Mutation},
        prelude::*,
    };
    fn app() -> Element {
        rsx! { super::SaveReport { rows: vec![], filename: "empty.csv" } }
    }
    let mut dom = VirtualDom::new(app);
    let mutations = dom.rebuild_to_vec();
    assert!(
        mutations.edits.iter().any(|edit| matches!(
            edit,
            Mutation::SetAttribute {
                name: "disabled",
                value: AttributeValue::Bool(true),
                ..
            }
        )),
        "An empty report must render a disabled save button"
    );
}

#[cfg(not(target_arch = "wasm32"))]
mod picker {
    use super::super::*;
    use dioxus::document::{Document, Eval, EvalError, Evaluator};
    use futures::FutureExt;
    use std::{
        cell::RefCell,
        collections::VecDeque,
        rc::Rc,
        task::{Context, Poll},
    };

    struct Replies {
        owner: dioxus::dioxus_core::internal::generational_box::Owner,
        replies: RefCell<VecDeque<serde_json::Value>>,
    }
    struct Message(Option<serde_json::Value>);
    impl Evaluator for Message {
        fn send(&self, _: serde_json::Value) -> Result<(), EvalError> {
            Ok(())
        }
        fn poll_recv(&mut self, _: &mut Context<'_>) -> Poll<Result<serde_json::Value, EvalError>> {
            Poll::Ready(self.0.take().ok_or(EvalError::Unsupported))
        }
        fn poll_join(
            &mut self,
            cx: &mut Context<'_>,
        ) -> Poll<Result<serde_json::Value, EvalError>> {
            self.poll_recv(cx)
        }
    }
    impl Document for Replies {
        fn eval(&self, _: String) -> Eval {
            Eval::new(
                self.owner
                    .insert(Box::new(Message(self.replies.borrow_mut().pop_front()))),
            )
        }
    }
    #[test]
    fn picker_cancellation_fallback_and_failure_keep_existing_outcomes() {
        for (reply, expected) in [
            (serde_json::json!({"kind":"cancelled"}), Ok(false)),
            (serde_json::json!({"kind":"unsupported"}), Ok(true)),
            (
                serde_json::json!({"kind":"failed","message":"Permission denied"}),
                Err("Permission denied".to_string()),
            ),
        ] {
            let document = Rc::new(Replies {
                owner: dioxus::dioxus_core::internal::generational_box::Owner::default(),
                replies: RefCell::new(VecDeque::from([reply])),
            });
            let dom = VirtualDom::new(|| rsx! {}).with_root_context(document as Rc<dyn Document>);
            let result = dom.in_scope(ScopeId::ROOT, || {
                finish_picker(begin_picker("test-save", "report.csv"), &[])
                    .now_or_never()
                    .expect("controlled reply")
            });
            assert_eq!(result, expected);
        }
    }
}

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

use super::encoded_bytes;
#[test]
fn binary_bridge_carries_bytes_as_base64_text() {
    assert_eq!(encoded_bytes(&[0, 1, 127, 128, 255]), "AAF/gP8=");
    assert_eq!(encoded_bytes(b"PK\x03\x04"), "UEsDBA==");
}

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

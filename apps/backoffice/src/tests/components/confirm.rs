use super::*;
use dioxus::dioxus_core::{AttributeValue, TemplateNode, VNode};

#[test]
fn confirmation_has_native_modal_semantics_and_instance_specific_labels() {
    let mut first = confirmation();
    let mut second = confirmation();
    first.rebuild_in_place();
    second.rebuild_in_place();
    let first = first
        .get_scope(ScopeId::APP)
        .expect("app scope")
        .root_node();
    let second = second
        .get_scope(ScopeId::APP)
        .expect("app scope")
        .root_node();
    assert!(
        matches!(
            first.template.roots[0],
            TemplateNode::Element { tag: "dialog", .. }
        ),
        "native dialog owns modality and focus containment"
    );
    let title = text_attribute(first, &[0], "aria-labelledby");
    let message = text_attribute(first, &[0], "aria-describedby");
    assert_eq!(title, text_attribute(first, &[0, 0], "id"));
    assert_eq!(message, text_attribute(first, &[0, 1], "id"));
    assert_ne!(title, text_attribute(second, &[0], "aria-labelledby"));
    assert_ne!(message, text_attribute(second, &[0], "aria-describedby"));
    assert!(
        first
            .template
            .attr_paths
            .iter()
            .zip(&first.dynamic_attrs)
            .any(|(path, attributes)| **path == [0, 2, 0]
                && attributes
                    .iter()
                    .any(|attribute| attribute.name == "autofocus"
                        && matches!(attribute.value, AttributeValue::Bool(true)))),
        "Cancel receives initial focus before the destructive action"
    );
}

fn confirmation() -> VirtualDom {
    fn app() -> Element {
        Confirm(ConfirmProps {
            open: true,
            title: "Delete admin".into(),
            message: "Permanently delete this admin?".into(),
            confirm_label: "Delete".into(),
            danger: true,
            on_confirm: EventHandler::default(),
            on_cancel: EventHandler::default(),
        })
    }
    VirtualDom::new(app)
}

fn text_attribute<'a>(node: &'a VNode, path: &[u8], name: &str) -> &'a str {
    node.template
        .attr_paths
        .iter()
        .zip(&node.dynamic_attrs)
        .filter(|(attribute_path, _)| **attribute_path == path)
        .flat_map(|(_, attributes)| attributes.iter())
        .find_map(|attribute| match &attribute.value {
            AttributeValue::Text(value) if attribute.name == name => Some(value.as_str()),
            _ => None,
        })
        .expect("accessible attribute must resolve to the element it labels")
}

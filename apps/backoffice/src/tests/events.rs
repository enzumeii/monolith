pub(crate) struct FormEventConverter;

struct TestFormValue {
    value: String,
    files: Vec<dioxus::html::FileData>,
}
impl dioxus::html::HasFileData for TestFormValue {
    fn files(&self) -> Vec<dioxus::html::FileData> {
        self.files.clone()
    }
}
impl dioxus::html::HasFormData for TestFormValue {
    fn value(&self) -> String {
        self.value.clone()
    }
    fn valid(&self) -> bool {
        true
    }
    fn values(&self) -> Vec<(String, dioxus::html::FormValue)> {
        vec![]
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
struct TestClick;
impl dioxus::html::InteractionLocation for TestClick {
    fn client_coordinates(&self) -> dioxus::html::geometry::ClientPoint {
        dioxus::html::geometry::ClientPoint::default()
    }
    fn screen_coordinates(&self) -> dioxus::html::geometry::ScreenPoint {
        dioxus::html::geometry::ScreenPoint::default()
    }
    fn page_coordinates(&self) -> dioxus::html::geometry::PagePoint {
        dioxus::html::geometry::PagePoint::default()
    }
}
impl dioxus::html::InteractionElementOffset for TestClick {
    fn element_coordinates(&self) -> dioxus::html::geometry::ElementPoint {
        dioxus::html::geometry::ElementPoint::default()
    }
}
impl dioxus::html::ModifiersInteraction for TestClick {
    fn modifiers(&self) -> dioxus::html::Modifiers {
        dioxus::html::Modifiers::empty()
    }
}
impl dioxus::html::PointerInteraction for TestClick {
    fn trigger_button(&self) -> Option<dioxus::html::input_data::MouseButton> {
        Some(dioxus::html::input_data::MouseButton::Primary)
    }
    fn held_buttons(&self) -> dioxus::html::input_data::MouseButtonSet {
        dioxus::html::input_data::MouseButtonSet::default()
    }
}
impl dioxus::html::HasMouseData for TestClick {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
macro_rules! form_event_converter {
    (enum Event { $(#[convert = $converter:ident] #[events = [$($(#[$attr:meta])* $name:ident => $raw:ident,)*]] $(#[raw = [$($raw_only:ident),* $(,)?]])? $group:ident($data:ident),)* }) => {
        impl dioxus::html::HtmlEventConverter for FormEventConverter {
            $(fn $converter(&self, _event: &dioxus::html::PlatformEventData) -> dioxus::html::$data {
                form_event_converter!(@data $data, _event)
            })*
        }
    };
    (@data MouseData, $event:ident) => { dioxus::html::MouseData::new(TestClick) };
    (@data FormData, $event:ident) => { dioxus::html::FormData::new(TestFormValue {
        value: $event.downcast::<String>().cloned().unwrap_or_default(),
        files: $event.downcast::<Vec<dioxus::html::FileData>>().cloned().unwrap_or_default(),
    }) };
    (@data $other:ident, $event:ident) => { panic!("unexpected non-form event: {}", stringify!($other)) };
}
dioxus::html::with_html_event_groups!(form_event_converter);

pub(crate) fn listeners(
    mutations: &dioxus::dioxus_core::Mutations,
    event: &str,
) -> Vec<dioxus::dioxus_core::ElementId> {
    mutations
        .edits
        .iter()
        .filter_map(|edit| match edit {
            dioxus::dioxus_core::Mutation::NewEventListener { name, id } if *name == event => {
                Some(*id)
            }
            _ => None,
        })
        .collect()
}

pub(crate) fn form_event(
    dom: &dioxus::prelude::VirtualDom,
    id: dioxus::dioxus_core::ElementId,
    name: &str,
    value: &str,
) {
    use std::rc::Rc;
    let data = dioxus::html::PlatformEventData::new(Box::new(value.to_owned()));
    dom.runtime().handle_event(
        name,
        dioxus::prelude::Event::new(Rc::new(data) as Rc<dyn std::any::Any>, true),
        id,
    );
}

pub(crate) fn disabled_state(
    mutations: &dioxus::dioxus_core::Mutations,
) -> Vec<(dioxus::dioxus_core::ElementId, bool)> {
    use dioxus::dioxus_core::{AttributeValue, Mutation};
    mutations
        .edits
        .iter()
        .filter_map(|edit| match edit {
            Mutation::SetAttribute {
                name: "disabled",
                value: AttributeValue::Bool(value),
                id,
                ..
            } => Some((*id, *value)),
            _ => None,
        })
        .collect()
}

pub(crate) fn render_changes(
    dom: &mut dioxus::prelude::VirtualDom,
) -> dioxus::dioxus_core::Mutations {
    use futures::FutureExt;
    let mut changes = dom.render_immediate_to_vec();
    for _ in 0..5 {
        let _ = dom.wait_for_work().now_or_never();
        changes.edits.extend(dom.render_immediate_to_vec().edits);
    }
    changes
}

pub(crate) fn element_id(
    dom: &dioxus::prelude::VirtualDom,
    value: &str,
) -> dioxus::dioxus_core::ElementId {
    find_element(dom, dom.base_scope().root_node(), "id", value, false)
        .expect("rendered element id")
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn dialog_button(
    dom: &dioxus::prelude::VirtualDom,
    class: &str,
) -> dioxus::dioxus_core::ElementId {
    find_element(dom, dom.base_scope().root_node(), "class", class, true)
        .expect("rendered dialog button")
}

fn find_element(
    dom: &dioxus::prelude::VirtualDom,
    vnode: &dioxus::dioxus_core::VNode,
    name: &str,
    value: &str,
    dialog_only: bool,
) -> Option<dioxus::dioxus_core::ElementId> {
    use dioxus::dioxus_core::DynamicNode;
    for (index, root) in vnode.template.roots.iter().enumerate() {
        if let Some(id) = find_template_element(
            dom,
            vnode,
            root,
            &[u8::try_from(index).expect("template root index")],
            (name, value),
            dialog_only,
            false,
        ) {
            return Some(id);
        }
    }
    vnode
        .dynamic_nodes
        .iter()
        .enumerate()
        .find_map(|(index, node)| match node {
            DynamicNode::Component(component) => component
                .mounted_scope(index, vnode, dom)
                .and_then(|scope| find_element(dom, scope.root_node(), name, value, dialog_only)),
            DynamicNode::Fragment(nodes) => nodes
                .iter()
                .find_map(|node| find_element(dom, node, name, value, dialog_only)),
            _ => None,
        })
}

fn find_template_element(
    dom: &dioxus::prelude::VirtualDom,
    vnode: &dioxus::dioxus_core::VNode,
    node: &dioxus::dioxus_core::TemplateNode,
    path: &[u8],
    attribute: (&str, &str),
    dialog_only: bool,
    in_dialog: bool,
) -> Option<dioxus::dioxus_core::ElementId> {
    use dioxus::dioxus_core::{AttributeValue, TemplateAttribute, TemplateNode};
    let TemplateNode::Element {
        tag,
        attrs,
        children,
        ..
    } = node
    else {
        return None;
    };
    let (name, value) = attribute;
    let in_dialog = in_dialog || *tag == "dialog";
    let matches = attrs.iter().any(|attr| match attr {
        TemplateAttribute::Static {
            name: actual_name,
            value: actual_value,
            ..
        } => *actual_name == name && *actual_value == value,
        TemplateAttribute::Dynamic { id } => vnode.dynamic_attrs[*id].iter().any(|attr| {
            attr.name == name
                && matches!(&attr.value, AttributeValue::Text(actual) if actual == value)
        }),
    });
    if matches && (!dialog_only || (in_dialog && *tag == "button")) {
        if let Some(index) = vnode
            .template
            .attr_paths
            .iter()
            .position(|actual| *actual == path)
        {
            return vnode.mounted_dynamic_attribute(index, dom);
        }
        if path.len() == 1 {
            return vnode.mounted_root(usize::from(path[0]), dom);
        }
    }
    children.iter().enumerate().find_map(|(index, child)| {
        let mut child_path = path.to_vec();
        child_path.push(u8::try_from(index).expect("template child index"));
        find_template_element(
            dom,
            vnode,
            child,
            &child_path,
            (name, value),
            dialog_only,
            in_dialog,
        )
    })
}

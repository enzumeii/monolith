pub(crate) struct FormEventConverter;

struct TestFormValue(String);
impl dioxus::html::HasFileData for TestFormValue {
    fn files(&self) -> Vec<dioxus::html::FileData> {
        vec![]
    }
}
impl dioxus::html::HasFormData for TestFormValue {
    fn value(&self) -> String {
        self.0.clone()
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
    (@data FormData, $event:ident) => { dioxus::html::FormData::new(TestFormValue($event.downcast::<String>().expect("form value").clone())) };
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
    mutations: &dioxus::dioxus_core::Mutations,
    value: &str,
) -> dioxus::dioxus_core::ElementId {
    use dioxus::dioxus_core::{AttributeValue, Mutation};
    mutations
        .edits
        .iter()
        .find_map(|edit| match edit {
            Mutation::SetAttribute {
                name: "id",
                value: AttributeValue::Text(actual),
                id,
                ..
            } if actual == value => Some(*id),
            _ => None,
        })
        .expect("rendered element id")
}

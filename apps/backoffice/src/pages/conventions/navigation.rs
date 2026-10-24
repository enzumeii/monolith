use crate::route::Route;
use dioxus::prelude::*;

#[component]
pub(super) fn ReturnToList(message: String) -> Element {
    let navigator = use_navigator();
    use_effect(move || {
        navigator.replace(Route::Conventions {
            query: String::new(),
        });
    });
    rsx! { p { role: "status", "{message}" } }
}

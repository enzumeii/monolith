use crate::components::SITE_LOGO;
use dioxus::prelude::*;

#[component]
pub fn Home() -> Element {
    rsx! {
        main { class: "page home-page", aria_label: "Enzumeii home",
            img { class: "home-logo", src: SITE_LOGO, alt: "Enzumeii Art & Merchandise", width: "320", height: "320" }
        }
    }
}

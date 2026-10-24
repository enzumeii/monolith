use crate::route::Route;
use dioxus::prelude::*;

#[component]
pub(super) fn PortalNavbar(children: Option<Element>) -> Element {
    rsx! {
        header { class: "portal-brand portal-navbar",
            Link { to: Route::Directory {}, class: "portal-brand-link", aria_label: "Enzumeii home",
                img { class: "portal-brand-logo", src: ui::SITE_LOGO, alt: "Enzumeii" }
                span { "Enzumeii" }
            }
            if let Some(children) = children {
                nav { class: "portal-nav-actions", aria_label: "Convention navigation", {children} }
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/pages/navbar.rs"]
mod tests;

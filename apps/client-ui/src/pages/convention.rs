use super::{ConventionHeader, navbar::PortalNavbar};
use crate::{
    api,
    cart::{CartContext, CartProvider, CartView},
    route::Route,
};
use dioxus::prelude::*;
use domain::dto::PublicCatalogueResponse;
use ui::{ErrorMessage, LoadingMessage};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ConventionView {
    Catalogue,
    Product { product_id: String },
    Cart { checkout_id: String },
}

impl ConventionView {
    pub(crate) fn back_route(&self, slug: &str) -> Option<Route> {
        match self {
            Self::Catalogue => None,
            Self::Product { .. } | Self::Cart { .. } => {
                Some(Route::CataloguePage { slug: slug.into() })
            }
        }
    }
}

#[component]
pub fn CartPage(slug: String, checkout_id: String) -> Element {
    rsx! { ConventionPage { key: "{slug}", slug, view: ConventionView::Cart { checkout_id } } }
}

#[component]
pub(crate) fn ConventionPage(slug: String, view: ConventionView) -> Element {
    let back_to = view.back_route(&slug);
    let catalogue = use_resource(move || {
        let slug = slug.clone();
        async move { api::retry(|| api::conventions::catalogue(&slug)).await }
    });
    let state = catalogue.read().clone();
    match state {
        None => rsx! {
            ConventionFrame { back_to, LoadingMessage { message: "Loading convention…" } }
        },
        Some(Err(error)) => rsx! {
            ConventionFrame { back_to,
                ErrorMessage { message: Some(if error.status == 404 { "This convention is not available for preorders.".into() } else { error.message }) }
            }
        },
        Some(Ok(catalogue)) => rsx! {
            CartProvider { key: "{catalogue.convention.id}", catalogue, view }
        },
    }
}

#[component]
pub(crate) fn ConventionFrame(
    children: Element,
    cart_action: Option<Element>,
    back_to: Option<Route>,
) -> Element {
    rsx! {
        PortalNavbar { children: cart_action }
        main { class: "page portal-page",
            if let Some(destination) = back_to {
                Link { to: destination, class: "button-link ghost portal-back-link", aria_label: "Back to products",
                    span { aria_hidden: "true", "<" }
                }
            }
            {children}
        }
    }
}

#[component]
pub(crate) fn ConventionContent(
    catalogue: PublicCatalogueResponse,
    view: ConventionView,
    notice: Option<String>,
) -> Element {
    let back_to = view.back_route(&catalogue.convention.slug);
    let context = use_context::<CartContext>();
    let cart = context.cart.read();
    let count: u32 = cart.items.iter().map(|item| item.quantity).sum();
    let (destination, label) = if matches!(view, ConventionView::Cart { .. }) {
        (
            Route::CataloguePage {
                slug: catalogue.convention.slug.clone(),
            },
            format!("Browse products ({count} items in cart)"),
        )
    } else {
        (
            Route::CartPage {
                slug: catalogue.convention.slug.clone(),
                checkout_id: cart.checkout_id.clone(),
            },
            format!("View cart ({count} items)"),
        )
    };
    rsx! {
        ConventionFrame { back_to,
            cart_action: rsx! {
                Link { to: destination, class: "button-link portal-cart-link", aria_label: label,
                    svg { class: "portal-cart-icon", width: "24", height: "24", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "1.75", stroke_linecap: "round", stroke_linejoin: "round", "aria-hidden": "true", "focusable": "false",
                        path { d: "M3 3h2l2.4 12h11.2l2-8H6" }
                        circle { cx: "9", cy: "20", r: "1" }
                        circle { cx: "18", cy: "20", r: "1" }
                    }
                    span { class: "portal-cart-count", aria_hidden: "true", "{count}" }
                }
            },
            ConventionHeader { convention: catalogue.convention.clone() }
            if let Some(message) = notice { p { class: "portal-notice", role: "status", "{message}" } }
            match view {
                ConventionView::Catalogue => rsx! { super::catalogue::Catalogue { catalogue } },
                ConventionView::Product { product_id } => rsx! { super::product::ProductDetail { catalogue, product_id } },
                ConventionView::Cart { checkout_id } => rsx! { CartView { catalogue, checkout_id } },
            }
        }
    }
}

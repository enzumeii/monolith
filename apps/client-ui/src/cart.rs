pub mod browser;
mod model;
mod view;

pub use model::{Cart, reconciliation};
pub use view::CartView;

use dioxus::prelude::*;
use domain::dto::PublicCatalogueResponse;
use std::collections::BTreeMap;
use ui::{ErrorMessage, LoadingMessage};

#[derive(Clone, Copy)]
pub struct CartContext {
    pub cart: Signal<Cart>,
    cache: Signal<BTreeMap<String, browser::Restored>>,
}

impl CartContext {
    pub fn update<T>(&mut self, update: impl FnOnce(&mut Cart) -> T) -> T {
        let result = update(&mut self.cart.write());
        let snapshot = self.cart.peek().clone();
        let mut cache = self.cache.write();
        cache
            .entry(snapshot.convention_id.clone())
            .and_modify(|saved| saved.cart = snapshot.clone())
            .or_insert(browser::Restored {
                cart: snapshot,
                notice: None,
            });
        result
    }
}

#[component]
pub fn CartProvider(catalogue: PublicCatalogueResponse, checkout_id: Option<String>) -> Element {
    let convention_id = catalogue.convention.id.clone();
    let cache = use_context::<Signal<BTreeMap<String, browser::Restored>>>();
    let restored = use_resource(move || {
        let convention_id = convention_id.clone();
        async move {
            if let Some(restored) = cache.peek().get(&convention_id).cloned() {
                return Ok(restored);
            }
            browser::restore(&convention_id).await
        }
    });
    let restored = restored.read().clone();
    match restored {
        None => rsx! { LoadingMessage { message: "Restoring your cart…" } },
        Some(Err(message)) => rsx! { ErrorMessage { message: Some(message) } },
        Some(Ok(restored)) => rsx! { ReadyCart { catalogue, checkout_id, restored } },
    }
}

#[component]
fn ReadyCart(
    catalogue: PublicCatalogueResponse,
    checkout_id: Option<String>,
    restored: browser::Restored,
) -> Element {
    let mut cache = use_context::<Signal<BTreeMap<String, browser::Restored>>>();
    let cart = use_signal(|| restored.cart);
    let mut notice = use_signal(|| restored.notice);
    use_context_provider(|| CartContext { cart, cache });
    let _saving = use_resource(move || {
        let snapshot = cart.read().clone();
        cache.write().insert(
            snapshot.convention_id.clone(),
            browser::Restored {
                cart: snapshot.clone(),
                notice: notice.peek().clone(),
            },
        );
        async move {
            if let Err(message) = browser::save(&snapshot).await {
                notice.set(Some(message));
            }
        }
    });
    rsx! {
        if let Some(message) = notice.read().clone() { p { class: "portal-notice", role: "status", "{message}" } }
        if let Some(checkout_id) = checkout_id {
            CartView { catalogue, checkout_id }
        } else {
            crate::pages::catalogue::Catalogue { catalogue }
        }
    }
}

#[cfg(test)]
#[path = "tests/cart/context.rs"]
pub(crate) mod tests;

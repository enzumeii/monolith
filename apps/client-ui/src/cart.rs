pub mod browser;
mod model;
mod view;

pub(crate) use model::valid_checkout_id;
pub use model::{Cart, reconciliation};
pub use view::CartView;

use crate::pages::{ConventionContent, ConventionFrame, ConventionView};
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

pub(crate) fn use_cart_cache() {
    use_context_provider(|| Signal::new(BTreeMap::<String, browser::Restored>::new()));
}

#[component]
pub(crate) fn CartProvider(catalogue: PublicCatalogueResponse, view: ConventionView) -> Element {
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
    let back_to = view.back_route(&catalogue.convention.slug);
    match restored {
        None => {
            rsx! { ConventionFrame { back_to, LoadingMessage { message: "Restoring your cart…" } } }
        }
        Some(Err(message)) => {
            rsx! { ConventionFrame { back_to, ErrorMessage { message: Some(message) } } }
        }
        Some(Ok(restored)) => rsx! { ReadyCart { catalogue, view, restored } },
    }
}

#[component]
fn ReadyCart(
    catalogue: PublicCatalogueResponse,
    view: ConventionView,
    restored: browser::Restored,
) -> Element {
    let mut cache = use_context::<Signal<BTreeMap<String, browser::Restored>>>();
    let mut cart = use_signal(|| restored.cart);
    let mut notice = use_signal(|| restored.notice);
    use_context_provider(|| CartContext { cart, cache });
    let _saving = use_resource(use_reactive((&view,), move |(_view,)| {
        let snapshot = cart.read().clone();
        cache.write().insert(
            snapshot.convention_id.clone(),
            browser::Restored {
                cart: snapshot.clone(),
                notice: notice.peek().clone(),
            },
        );
        async move {
            match browser::save(&snapshot).await {
                Ok(saved) if saved.checkout_id != snapshot.checkout_id => {
                    if cart.peek().checkout_id == snapshot.checkout_id {
                        cart.set(saved);
                        notice.set(Some("Your cart changed in another tab. Review your current selections before continuing.".into()));
                    }
                }
                Err(message) => notice.set(Some(message)),
                Ok(_) => {}
            }
        }
    }));
    rsx! { ConventionContent { catalogue, view, notice: notice.read().clone() } }
}

#[cfg(test)]
#[path = "tests/cart/context.rs"]
pub(crate) mod tests;

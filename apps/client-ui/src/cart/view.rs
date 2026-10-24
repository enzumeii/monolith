use super::{Cart, CartContext, reconciliation};
use crate::{api, route::Route};
use dioxus::prelude::*;
use domain::dto::{ConventionQuote, PublicCatalogueResponse};
use ui::{ErrorMessage, Heading, HeadingLevel, LoadingMessage};

#[derive(Clone)]
struct CurrentCart {
    snapshot: Cart,
    catalogue: PublicCatalogueResponse,
    quote: Option<Result<ConventionQuote, api::ApiError>>,
}

#[component]
pub fn CartView(catalogue: PublicCatalogueResponse, checkout_id: String) -> Element {
    let mut context = use_context::<CartContext>();
    let slug = catalogue.convention.slug.clone();
    let currency = catalogue.convention.currency_code.clone();
    let mut current = use_resource(use_reactive(
        (&checkout_id, &slug, &currency),
        move |(requested_id, slug, expected_currency)| {
            let snapshot = context.cart.read().clone();
            let matches = requested_id == snapshot.checkout_id;
            async move {
                if !matches {
                    return Ok(None);
                }
                let catalogue = api::conventions::catalogue(&slug).await?;
                if catalogue.convention.id != snapshot.convention_id
                    || catalogue.convention.currency_code != expected_currency
                {
                    return Err(api::ApiError {
                        status: 409,
                        message: "This convention has changed. Return to its collection to start a separate cart.".into(),
                    });
                }
                let lines = reconciliation(&snapshot, &catalogue.products);
                let quote = if lines
                    .iter()
                    .any(|line| line.issue.is_some() || line.price_changed)
                {
                    None
                } else {
                    Some(api::conventions::quote(&slug, snapshot.quote_request()).await)
                };
                let mut catalogue = if quote.as_ref().is_some_and(|quote| {
                    quote.is_ok() || quote.as_ref().is_err_and(|error| error.status == 409)
                }) {
                    api::conventions::catalogue(&slug).await?
                } else {
                    catalogue
                };
                if catalogue.convention.id != snapshot.convention_id
                    || catalogue.convention.currency_code != expected_currency
                {
                    return Err(api::ApiError {
                        status: 409,
                        message: "This convention has changed. Return to its collection to start a separate cart.".into(),
                    });
                }
                if let Some(Ok(quote)) = &quote
                    && !super::model::apply_quote_prices(
                        &snapshot,
                        &expected_currency,
                        &mut catalogue,
                        quote,
                    )
                {
                    return Err(api::ApiError {
                        status: 502,
                        message: "The shop returned an invalid cart quote. Please retry.".into(),
                    });
                }
                Ok::<_, api::ApiError>(Some(CurrentCart {
                    snapshot,
                    catalogue,
                    quote,
                }))
            }
        },
    ));
    let cart = context.cart.read().clone();
    let active_route = Route::CartPage {
        slug: catalogue.convention.slug.clone(),
        checkout_id: cart.checkout_id.clone(),
    };
    if checkout_id != cart.checkout_id {
        return rsx! {
            Heading { title: "Cart unavailable", level: HeadingLevel::Section }
            p { "This cart does not match the cart saved in this browser for this convention." }
            Link { to: active_route, class: "button-link primary", "Open your current cart" }
            Link { to: Route::CataloguePage { slug: catalogue.convention.slug }, class: "button-link ghost", "Browse products" }
        };
    }
    let ready = *current.state().read() == UseResourceState::Ready;
    let state = current.read().clone();
    let fresh = if ready {
        state
            .as_ref()
            .and_then(|state| state.as_ref().ok())
            .and_then(Option::as_ref)
            .filter(|value| value.snapshot == cart)
    } else {
        None
    };
    let products = fresh.map_or(&catalogue.products, |value| &value.catalogue.products);
    let lines = reconciliation(&cart, products);
    let changed = lines.iter().any(|line| line.price_changed);
    let unresolved = changed || lines.iter().any(|line| line.issue.is_some());
    let current_products = products.clone();
    rsx! {
        Heading { title: "Your cart", level: HeadingLevel::Section }
        p { class: "meta", "Review your selections. Payment will be available later." }
        if lines.is_empty() { p { "Your cart is empty." } }
        div { class: "portal-cart-lines",
            for line in lines {
                CartLine { key: "{line.variant_id}", line, currency: currency.clone() }
            }
        }
        if !ready { LoadingMessage { message: "Checking current prices and availability…" } }
        if ready {
            if let Some(Err(error)) = state.as_ref() {
                ErrorMessage { message: Some(error.message.clone()) }
                button { class: "ghost", onclick: move |_| current.restart(), "Retry cart" }
            }
            if let Some(value) = fresh {
                if changed {
                    p { class: "portal-notice", role: "status", "Prices have changed since you added these items. Review and accept the current prices to update your total." }
                    button { class: "primary", onclick: move |_| context.update(|cart| cart.accept_prices(&current_products)), "Accept updated prices" }
                }
                match value.quote.as_ref() {
                    Some(Ok(quote)) if !unresolved => rsx! { QuoteSummary { quote: quote.clone() } },
                    Some(Err(error)) => rsx! {
                        ErrorMessage { message: Some(error.message.clone()) }
                        button { class: "ghost", onclick: move |_| current.restart(), "Retry cart" }
                    },
                    Some(Ok(_)) | None => rsx! { p { class: "hint", "Resolve the highlighted changes to see your current total." } },
                }
            }
        }
    }
}

#[component]
fn CartLine(line: super::model::ReconciledLine, currency: String) -> Element {
    let mut context = use_context::<CartContext>();
    let id = line.variant_id.clone();
    let remove_id = id.clone();
    let reduce_id = id.clone();
    let maximum = line.max_quantity;
    let label = format!("Quantity for {} / {}", line.title, line.variant_title);
    let input_id = format!("cart-quantity-{}", id.rsplit('/').next().unwrap_or("item"));
    let remove_label = format!("Remove {} / {}", line.title, line.variant_title);
    rsx! { article { class: "panel portal-cart-line",
        if let Some(image) = line.image.as_ref().filter(|image| crate::pages::safe_image(&image.url)) {
            img { class: "portal-cart-image", src: "{image.url}", alt: "{line.title} / {line.variant_title}" }
        } else {
            div { class: "portal-cart-image portal-cart-image-placeholder", role: "img", aria_label: "No image available", "✦" }
        }
        div { h3 { "{line.title}" } p { "{line.variant_title}" }
            if let Some(price) = line.current_price { p { "{currency} {price} each" } }
            if line.price_changed { p { class: "portal-notice", "Unit price changed. Acknowledgement required." } }
            ErrorMessage { message: line.issue }
        }
        div {
            label { r#for: "{input_id}", "{label}" }
            input { id: "{input_id}", r#type: "number", min: "1", max: "999", step: "1", value: "{line.quantity}",
                oninput: move |event| { if let Ok(quantity) = event.value().parse() { context.update(|cart| cart.set_quantity(&id, quantity)); } } }
            if maximum > 0 && line.quantity > maximum {
                button { class: "ghost", onclick: move |_| context.update(|cart| cart.set_quantity(&reduce_id, maximum)), "Reduce to {maximum}" }
            }
            button { class: "ghost", aria_label: remove_label, onclick: move |_| context.update(|cart| cart.remove(&remove_id)), "Remove" }
        }
    } }
}

#[component]
fn QuoteSummary(quote: ConventionQuote) -> Element {
    rsx! { section { class: "panel portal-totals", aria_label: "Current cart total",
        h3 { "Current total" }
        for line in quote.items { p { "{line.quantity} × {quote.currency_code} {line.unit_price} = {quote.currency_code} {line.line_total}" } }
        dl {
            dt { "Subtotal" } dd { "{quote.currency_code} {quote.subtotal}" }
            dt { "Total" } dd { strong { "{quote.currency_code} {quote.total}" } }
        }
    } }
}

#[cfg(test)]
#[path = "../tests/cart/view.rs"]
mod tests;

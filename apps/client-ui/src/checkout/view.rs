use super::{
    browser,
    model::{self, Attempt},
};
use crate::{api, cart::Cart, pages::ConventionFrame, route::Route};
use dioxus::prelude::*;
use domain::dto::{
    ConventionQuote,
    checkout::{CheckoutProvider, CheckoutResponse, CheckoutStatus},
};
use ui::{ErrorMessage, Heading, LoadingMessage};

#[derive(Clone, Default, PartialEq)]
struct Observation {
    response: Option<CheckoutResponse>,
    error: Option<String>,
    checking: bool,
}

struct AttemptError {
    message: String,
    disposition: Option<&'static str>,
}

#[component]
pub fn CheckoutControls(
    slug: String,
    snapshot: Cart,
    quote: Option<ConventionQuote>,
    busy: Signal<bool>,
) -> Element {
    let mut email = use_signal(String::new);
    let error = use_signal(|| None::<String>);
    let attempt = use_signal(|| None::<Attempt>);
    let saved_slug = slug.clone();
    let convention = snapshot.convention_id.clone();
    let saved = use_resource(move || {
        let slug = saved_slug.clone();
        let convention = convention.clone();
        async move { browser::read(&slug, Some(&convention), None).await }
    });
    let saved_state = saved.read().clone();
    let existing = attempt.read().clone().or_else(|| {
        saved_state
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .and_then(Clone::clone)
    });
    if let Some(existing) =
        existing.filter(|a| !matches!(a.state.as_str(), "completed" | "unpaid" | "rejected"))
    {
        return rsx! {
            section { class: "panel checkout-status",
                h3 { "A payment has already been started" }
                p { "Check your original payment before paying again. Your cart is saved." }
                ErrorMessage { message: error.read().clone() }
                Link { to: Route::CheckoutReturnPage { slug: existing.slug.clone(), query: format!("checkout_id={}", existing.operation_id) }, class: "button-link primary", "Check existing payment" }
            }
        };
    }
    let unavailable = snapshot.items.is_empty()
        || quote.is_none()
        || !model::valid_email(&email.read())
        || *busy.read()
        || !matches!(saved_state, Some(Ok(_)));
    let paypal_slug = slug.clone();
    let paypal_snapshot = snapshot.clone();
    let paypal_quote = quote.clone();
    let paypal_email = email.read().clone();
    let card_email = paypal_email.clone();
    rsx! {
        section { class: "panel checkout-controls", aria_label: "Payment",
            h3 { "Payment" }
            label { r#for: "checkout-email", "Confirmation email" }
            input { id: "checkout-email", r#type: "email", autocomplete: "email", required: true,
                value: "{email}", disabled: *busy.read(),
                oninput: move |event| email.set(event.value()),
                placeholder: "you@example.com" }
            p { class: "hint", "Your order confirmation will be sent to this email. Show it at the convention to confirm your preorder." }
            if !email.read().is_empty() && !model::valid_email(&email.read()) { p { class: "hint", "Enter a valid email address to continue." } }
            button { id: "checkout-paypal", class: "checkout-payment-button checkout-paypal", disabled: unavailable,
                onclick: move |_| {
                    if *busy.peek() { return; }
                    busy.set(true);
                    let slug = paypal_slug.clone(); let snapshot = paypal_snapshot.clone(); let quote = paypal_quote.clone(); let email = paypal_email.clone();
                    spawn(async move { begin(slug, snapshot, quote, email, CheckoutProvider::Paypal, (busy, error, attempt)).await; });
                },
                PaymentMarks { provider: CheckoutProvider::Paypal }
                span { "Pay with PayPal" }
            }
            button { id: "checkout-card", class: "checkout-payment-button checkout-card", disabled: unavailable,
                onclick: move |_| {
                    if *busy.peek() { return; }
                    busy.set(true);
                    let slug = slug.clone(); let snapshot = snapshot.clone(); let quote = quote.clone(); let email = card_email.clone();
                    spawn(async move { begin(slug, snapshot, quote, email, CheckoutProvider::Stripe, (busy, error, attempt)).await; });
                },
                PaymentMarks { provider: CheckoutProvider::Stripe }
                span { "Pay with credit card, Google Pay or Apple Pay" }
            }
            p { class: "hint", "Wallet availability depends on your device and the payment gateway." }
            if *busy.read() { LoadingMessage { message: "Preparing your secure payment…" } }
            ErrorMessage { message: error.read().clone().or_else(|| saved_state.and_then(Result::err)) }
        }
    }
}

#[component]
pub fn CheckoutReturnPage(slug: String, query: String) -> Element {
    rsx! { CheckoutStatusPage { slug, query, cancelled: false } }
}

#[component]
pub fn CheckoutCancelPage(slug: String, query: String) -> Element {
    rsx! { CheckoutStatusPage { slug, query, cancelled: true } }
}

#[component]
fn CheckoutStatusPage(slug: String, query: String, cancelled: bool) -> Element {
    let back_to = Some(Route::CataloguePage { slug: slug.clone() });
    let recovery = use_resource(use_reactive((&slug, &query), |(slug, query)| async move {
        let ids: Vec<_> = reqwest::Url::parse(&format!(
            "https://checkout.invalid/?{}",
            query.trim_start_matches('?')
        ))
        .map_err(|_| "This checkout link is invalid.")?
        .query_pairs()
        .filter(|(key, _)| key == "checkout_id")
        .map(|(_, value)| value.into_owned())
        .collect();
        if ids.len() != 1 || !model::valid_id(&ids[0]) {
            return Err(
                "This checkout link is invalid. Keep any payment receipt and contact the shop."
                    .into(),
            );
        }
        browser::read(&slug, None, Some(&ids[0])).await?.ok_or_else(|| "This browser has no recovery information for this checkout. Open this link in the browser used to pay. If it is unavailable, keep your payment receipt and contact the shop before paying again.".into())
    }));
    let state = recovery.read().clone();
    rsx! { ConventionFrame { back_to,
        match state {
            None => rsx! { LoadingMessage { message: "Opening your checkout…" } },
            Some(Err(message)) => rsx! { Heading { title: "Payment recovery" } ErrorMessage { message: Some(message) } },
            Some(Ok(attempt)) => rsx! { PaymentSession { key: "{attempt.operation_id}", attempt, cancelled } },
        }
    } }
}

#[component]
fn PaymentSession(attempt: Attempt, cancelled: bool) -> Element {
    let mut observation = use_signal(Observation::default);
    let polling_attempt = attempt.clone();
    let mut polling = use_resource(move || {
        let attempt = polling_attempt.clone();
        async move {
            observation.write().checking = true;
            observation.write().error = None;
            for check in 0..12 {
                match inspect(&attempt).await {
                    Ok(response) => {
                        let pending = response.status == CheckoutStatus::Processing;
                        observation.set(Observation {
                            response: Some(response),
                            error: None,
                            checking: pending,
                        });
                        if !pending || check == 11 || !browser::delay().await {
                            break;
                        }
                    }
                    Err(message) => {
                        observation.write().error = Some(message);
                        break;
                    }
                }
            }
            observation.write().checking = false;
        }
    });
    let state = observation.read().clone();
    let retry = move |_| {
        if !observation.peek().checking {
            polling.restart();
        }
    };
    rsx! {
        section { class: "panel checkout-status", aria_live: "polite",
            if cancelled { p { class: "portal-notice", "You left the payment page. That does not confirm cancellation. We are checking the original payment; an already approved PayPal payment may still complete." } }
            if let Some(response) = state.response {
                StatusContent { response: response.clone(), provider: attempt.provider }
                if response.status == CheckoutStatus::Unpaid {
                    Link { to: Route::CartPage { slug: attempt.slug.clone(), checkout_id: attempt.snapshot.checkout_id.clone() }, class: "button-link ghost", "Return to cart" }
                }
            } else if state.error.is_none() { LoadingMessage { message: "Checking your payment and order…" } }
            ErrorMessage { message: state.error }
            if state.checking { p { class: "hint", "Checking automatically. You can safely keep this page open." } }
            button { class: "ghost", disabled: state.checking, onclick: retry, "Check status" }
        }
    }
}

#[component]
fn StatusContent(response: CheckoutResponse, provider: CheckoutProvider) -> Element {
    let mut navigation_error = use_signal(|| None::<String>);
    match response.status {
        CheckoutStatus::Completed => {
            let order = response
                .order
                .expect("completed response checked before rendering");
            rsx! {
                Heading { title: "Congratulations! Your order is confirmed." }
                p { class: "checkout-order-reference", "Shopify order ", strong { "{order.name}" } }
                p { "Check your email for your order confirmation. Show that email at the convention to confirm your preorder." }
            }
        }
        CheckoutStatus::RequiresAction => rsx! {
            Heading { title: "Continue your payment" }
            p { "Your original checkout is ready. Continue with the same payment to finish your preorder." }
            button { class: "primary", onclick: move |_| {
                let response = response.clone();
                spawn(async move {
                    let result = hosted_redirect(&response, provider).await;
                    if let Err(message) = result { navigation_error.set(Some(message)); }
                });
            }, "Continue payment" }
            ErrorMessage { message: navigation_error.read().clone() }
        },
        CheckoutStatus::Processing => rsx! {
            Heading { title: "Your payment is being checked" }
            p { "We are confirming your payment and preparing your order. Please do not pay again. Your cart is saved." }
        },
        CheckoutStatus::Unpaid => rsx! {
            Heading { title: "Payment was not completed" }
            p { "The payment gateway confirmed this payment is unpaid. Your cart is saved. Return to your cart when you are ready to try again." }
        },
        CheckoutStatus::ReconciliationRequired => rsx! {
            Heading { title: "Your order needs a payment check" }
            p { "Please do not pay again. Keep your payment receipt and contact the shop so we can confirm your preorder. Your cart is saved." }
        },
    }
}

#[component]
fn PaymentMarks(provider: CheckoutProvider) -> Element {
    match provider {
        CheckoutProvider::Paypal => rsx! {
            svg { class: "checkout-paypal-mark", width: "30", height: "32", view_box: "0 0 32 34", "aria-hidden": "true", "focusable": "false",
                path { fill: "#003087", d: "M8 1h12c8 0 10 5 8 11-2 6-6 8-12 8h-4l-2 12H3z" }
                path { fill: "#009cde", d: "M13 8h10c7 0 9 4 7 10-2 6-6 8-11 8h-3l-1 7H8z", opacity: ".95" }
                path { fill: "#012169", d: "M13 8h7c4 0 7 1 8 3-1 6-6 9-12 9h-5z" }
            }
        },
        CheckoutProvider::Stripe => rsx! {
            span { class: "checkout-wallet-marks", aria_hidden: "true",
                svg { width: "30", height: "24", view_box: "0 0 30 24", fill: "none", stroke: "currentColor", stroke_width: "1.8", "focusable": "false",
                    rect { x: "1", y: "2", width: "28", height: "20", rx: "3" }
                    path { d: "M1 8h28M6 16h6" }
                }
                svg { width: "46", height: "24", view_box: "0 0 46 24", "focusable": "false",
                    path { fill: "#4285f4", d: "M14 12.2c0-.5 0-1-.1-1.5H7.5v2.6h3.6a3 3 0 0 1-1.4 2v2.1h2.2c1.3-1.2 2.1-3 2.1-5.2" }
                    path { fill: "#34a853", d: "M7.5 19c1.9 0 3.5-.6 4.6-1.6l-2.2-2.1c-.6.4-1.3.6-2.4.6-1.8 0-3.3-1.2-3.9-2.8H1.3v2.2A7 7 0 0 0 7.5 19" }
                    path { fill: "#fbbc05", d: "M3.6 13.1a4.3 4.3 0 0 1 0-2.6V8.3H1.3a7 7 0 0 0 0 7z" }
                    path { fill: "#ea4335", d: "M7.5 7.6c1 0 1.8.3 2.5 1l2-2a7 7 0 0 0-10.7 1.7l2.3 2.2a4.1 4.1 0 0 1 3.9-2.9" }
                    text { x: "17", y: "17", fill: "currentColor", font_size: "12", font_family: "Arial, sans-serif", "Pay" }
                }
                svg { width: "49", height: "24", view_box: "0 0 49 24", fill: "currentColor", "focusable": "false",
                    path { d: "M12 5c.8-1 1.3-2.1 1.2-3.3-1.2 0-2.6.8-3.4 1.7-.7.8-1.4 2.1-1.2 3.3 1.3.1 2.6-.6 3.4-1.7M15.4 12.6c0-2 1.7-3 1.8-3.1-1-1.5-2.6-1.7-3.2-1.7-1.4-.1-2.7.8-3.4.8-.7 0-1.8-.8-3-.8-1.6 0-3 1-3.8 2.3-1.6 2.8-.4 6.8 1.2 9.1.7 1.1 1.6 2.3 2.8 2.2 1.2 0 1.6-.7 3.1-.7s1.9.7 3.1.7c1.3 0 2.1-1.1 2.8-2.2.9-1.2 1.2-2.4 1.2-2.5-.1 0-2.6-1-2.6-4.1" }
                    text { x: "21", y: "18", font_size: "14", font_family: "Arial, sans-serif", "Pay" }
                }
            }
        },
    }
}

async fn begin(
    slug: String,
    snapshot: Cart,
    quote: Option<ConventionQuote>,
    email: String,
    provider: CheckoutProvider,
    state: (
        Signal<bool>,
        Signal<Option<String>>,
        Signal<Option<Attempt>>,
    ),
) {
    let (mut busy, mut error, mut attempt) = state;
    error.set(None);
    let result = async {
        let quote = quote.ok_or("Review your current cart total before paying.")?;
        let request = model::request(&snapshot, &quote, email.trim(), provider)?;
        let (ownership, started) = browser::start(&slug, &snapshot, &request, &quote.total).await?;
        let mut saved = started.attempt.clone();
        let fresh = started.fresh;
        let result = advance(started).await;
        let status = result
            .as_ref()
            .map_or_else(|e| e.disposition, |r| Some(status_name(r.status)));
        ownership.finish(status).await?;
        if status == Some("rejected") {
            saved.state = "rejected".into();
        }
        attempt.set(Some(saved.clone()));
        let response = result.map_err(|e| e.message)?;
        if fresh && response.status == CheckoutStatus::RequiresAction {
            hosted_redirect(&response, saved.provider).await?;
        } else {
            browser::navigate(&format!(
                "/{}/checkout/return?checkout_id={}",
                saved.slug, saved.operation_id
            ))
            .await?;
        }
        Ok::<_, String>(())
    }
    .await;
    busy.set(false);
    if let Err(message) = result {
        error.set(Some(message));
    }
}

async fn inspect(attempt: &Attempt) -> Result<CheckoutResponse, String> {
    let (ownership, started) = browser::recover(attempt).await?;
    let result = advance(started).await;
    let status = result
        .as_ref()
        .map_or_else(|e| e.disposition, |r| Some(status_name(r.status)));
    ownership.finish(status).await?;
    if status == Some("completed") {
        invalidate_completed_cart(attempt);
    }
    result.map_err(|e| e.message)
}

async fn advance(started: browser::Started) -> Result<CheckoutResponse, AttemptError> {
    let attempt = &started.attempt;
    let result = if started.fresh {
        create_saved(&started).await
    } else {
        match api::orders::verify(&attempt.operation_id, &attempt.secret).await {
            Err(error) if matches!(error.status, 401 | 404) && started.payload.is_some() => {
                create_saved(&started).await
            }
            result => result,
        }
    };
    let response = result.map_err(|error| {
        let expired = matches!(error.status, 401 | 403);
        let rejected = matches!(error.status, 400 | 422);
        AttemptError {
            message: if expired {
                "This browser can no longer verify this checkout. Keep your payment receipt and contact the shop before paying again."
            } else if rejected {
                "Payment could not be started. Review your confirmation email and refresh your cart before trying again."
            } else {
                "We could not confirm this payment yet. Please do not pay again. Check the same checkout again, or contact the shop with your receipt."
            }.into(),
            disposition: if expired { Some("expired") } else if rejected && started.fresh { Some("rejected") } else { None },
        }
    })?;
    model::checked_response(attempt, &response).map_err(|message| AttemptError {
        message,
        disposition: None,
    })?;
    Ok(response)
}

async fn create_saved(started: &browser::Started) -> api::ApiResult<CheckoutResponse> {
    let payload = started.payload.as_deref().ok_or_else(|| api::ApiError {
        status: 409,
        message: "The original checkout details are no longer available in this tab.".into(),
    })?;
    let request: domain::dto::checkout::CreateCheckoutRequest = serde_json::from_str(payload)
        .map_err(|_| api::ApiError {
            status: 409,
            message: "The original checkout details cannot be restored.".into(),
        })?;
    if !model::saved_request_matches(&started.attempt, &request) {
        return Err(api::ApiError {
            status: 409,
            message: "The original checkout details do not match.".into(),
        });
    }
    api::orders::create(&request, &started.attempt.secret).await
}

fn invalidate_completed_cart(attempt: &Attempt) {
    if let Some(mut cache) = try_consume_context::<
        Signal<std::collections::BTreeMap<String, crate::cart::browser::Restored>>,
    >() {
        let matches = cache
            .peek()
            .get(&attempt.snapshot.convention_id)
            .is_some_and(|saved| saved.cart == attempt.snapshot);
        if matches {
            cache.write().remove(&attempt.snapshot.convention_id);
        }
    }
}

async fn hosted_redirect(
    response: &CheckoutResponse,
    provider: CheckoutProvider,
) -> Result<(), String> {
    let settings = try_consume_context::<crate::config::Settings>();
    let mut page = document::eval("dioxus.send(window.location.href);");
    let page = page
        .recv::<String>()
        .await
        .ok()
        .and_then(|url| reqwest::Url::parse(&url).ok());
    let local_api = settings
        .and_then(|s| reqwest::Url::parse(&s.api.base_url).ok())
        .is_some_and(|url| matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")));
    let test_origin = page
        .filter(|url| {
            local_api && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
        })
        .and(response.next_action.as_ref())
        .and_then(|action| match action {
            domain::dto::checkout::CheckoutNextAction::Redirect { url } => {
                reqwest::Url::parse(url).ok()
            }
        })
        .filter(|url| {
            url.scheme() == "https"
                && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
        })
        .map(|url| url.origin().ascii_serialization());
    let url = model::redirect(response, provider, test_origin.as_deref())?
        .ok_or("This payment does not require a redirect.")?;
    browser::navigate(&url).await
}

fn status_name(status: CheckoutStatus) -> &'static str {
    match status {
        CheckoutStatus::RequiresAction => "requires_action",
        CheckoutStatus::Processing => "processing",
        CheckoutStatus::Unpaid => "unpaid",
        CheckoutStatus::ReconciliationRequired => "reconciliation_required",
        CheckoutStatus::Completed => "completed",
    }
}

#[cfg(test)]
#[path = "../tests/checkout/view.rs"]
mod tests;

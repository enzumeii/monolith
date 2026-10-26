use super::{
    browser,
    model::{self, Attempt},
};
use crate::{api, cart::Cart, pages::ConventionFrame, route::Route};
use dioxus::prelude::*;
use domain::dto::{
    ConventionPaymentMethod, ConventionQuote,
    checkout::{CheckoutProvider, CheckoutResponse, CheckoutStatus},
};
use ui::{ErrorMessage, Heading, LoadingMessage};

#[derive(Clone, Default, PartialEq)]
struct Observation {
    response: Option<CheckoutResponse>,
    error: Option<String>,
}

type Coordination = Option<Result<browser::CheckingState, String>>;

#[derive(Clone, Copy)]
struct SubmissionState {
    busy: Signal<bool>,
    error: Signal<Option<String>>,
    attempt: Signal<Option<Attempt>>,
    delayed_recovery: Signal<bool>,
    submitting: Signal<bool>,
    provider_open: Signal<bool>,
}

#[derive(Clone)]
struct CustomerDetails {
    email: String,
    note: String,
}

struct AttemptError {
    message: String,
    disposition: Option<&'static str>,
    retryable: bool,
    ignored: bool,
}

#[component]
pub fn CheckoutControls(
    slug: String,
    snapshot: Cart,
    quote: Option<ConventionQuote>,
    payment_methods: Vec<ConventionPaymentMethod>,
    busy: Signal<bool>,
) -> Element {
    let mut email = use_signal(String::new);
    let mut note = use_signal(String::new);
    let error = use_signal(|| None::<String>);
    let attempt = use_signal(|| None::<Attempt>);
    let delayed_recovery = use_signal(|| false);
    let mut submitting = use_signal(|| false);
    let provider_open = use_signal(|| false);
    let saved_slug = slug.clone();
    let convention = snapshot.convention_id.clone();
    let saved = use_resource(move || {
        let slug = saved_slug.clone();
        let convention = convention.clone();
        async move { browser::read(&slug, Some(&convention), None).await }
    });
    use_effect(move || {
        let active = attempt.read().clone().or_else(|| {
            saved
                .read()
                .as_ref()
                .and_then(|value| value.as_ref().ok())
                .and_then(Clone::clone)
        });
        if active.is_some_and(|attempt| {
            !matches!(attempt.state.as_str(), "completed" | "unpaid" | "rejected")
        }) {
            busy.set(true);
        }
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
                if *submitting.read() {
                    LoadingMessage { message: "Preparing your secure payment…" }
                } else if *provider_open.read() {
                    PaymentSession {
                        key: "{existing.operation_id}",
                        attempt: existing,
                        cancelled: false,
                        awaiting_provider: true,
                    }
                } else {
                    ErrorMessage {
                        key: "{existing.operation_id}",
                        message: error.read().clone(),
                    }
                    RecoverSavedPayment {
                        attempt: existing,
                        delayed: *delayed_recovery.read(),
                        manual: error.read().is_some(),
                    }
                }
            }
        };
    }
    let payment_total = quote
        .as_ref()
        .map(|quote| format!("{} {}", quote.currency_code, quote.total));
    let paypal_label = payment_total.as_ref().map_or_else(
        || "Pay via PayPal".into(),
        |total| format!("Pay {total} via PayPal"),
    );
    let card_label = payment_total.as_ref().map_or_else(
        || "Pay by credit card".into(),
        |total| format!("Pay {total} by credit card"),
    );
    let note_error = model::validate_note(&note.read()).err();
    let unavailable = snapshot.items.is_empty()
        || quote.is_none()
        || !model::valid_email(&email.read())
        || note_error.is_some()
        || *busy.read()
        || !matches!(saved_state, Some(Ok(_)));
    let paypal_slug = slug.clone();
    let paypal_snapshot = snapshot.clone();
    let paypal_quote = quote.clone();
    let paypal_customer = CustomerDetails {
        email: email.read().clone(),
        note: note.read().clone(),
    };
    let card_customer = paypal_customer.clone();
    rsx! {
        section { class: "panel checkout-controls", aria_label: "Payment",
            h3 { "Payment" }
            label { r#for: "checkout-email", "Confirmation email" }
            input {
                id: "checkout-email",
                r#type: "email",
                autocomplete: "email",
                required: true,
                value: "{email}",
                disabled: *busy.read(),
                oninput: move |event| email.set(event.value()),
                placeholder: "you@example.com",
            }
            p { class: "hint",
                "Your order confirmation will be sent to this email. Show it at the convention to confirm your preorder."
            }
            if !email.read().is_empty() && !model::valid_email(&email.read()) {
                p { class: "hint", "Enter a valid email address to continue." }
            }
            label { r#for: "checkout-note", "Order note (optional)" }
            textarea {
                id: "checkout-note",
                rows: "3",
                value: "{note}",
                disabled: *busy.read(),
                placeholder: "Pickup day 1. Freebies character request : Zhongli",
                aria_describedby: if note_error.is_some() { "checkout-note-hint checkout-note-error" } else { "checkout-note-hint" },
                aria_invalid: note_error.is_some(),
                oninput: move |event| note.set(event.value()),
            }
            p { id: "checkout-note-hint", class: "hint",
                "Anything we should know about your order? Up to 2,000 characters."
            }
            if let Some(message) = note_error {
                p { id: "checkout-note-error", class: "hint", role: "alert", "{message}" }
            }
            div { class: "checkout-payment-methods",
                if payment_methods.contains(&ConventionPaymentMethod::Paypal) {
                    button {
                        id: "checkout-paypal",
                        class: "checkout-payment-button checkout-paypal",
                        disabled: unavailable,
                        onclick: move |_| {
                            if *busy.peek() {
                                return;
                            }
                            let window = browser::PaymentWindow::reserve();
                            busy.set(true);
                            submitting.set(true);
                            let slug = paypal_slug.clone();
                            let snapshot = paypal_snapshot.clone();
                            let quote = paypal_quote.clone();
                            let customer = paypal_customer.clone();
                            spawn(async move {
                                begin(
                                        slug,
                                        snapshot,
                                        quote,
                                        customer,
                                        CheckoutProvider::Paypal,
                                        SubmissionState {
                                            busy,
                                            error,
                                            attempt,
                                            delayed_recovery,
                                            submitting,
                                            provider_open,
                                        },
                                        window,
                                    )
                                    .await;
                            });
                        },
                        span { class: "checkout-payment-marks",
                            PaymentMarks { provider: CheckoutProvider::Paypal }
                        }
                        span { "{paypal_label}" }
                    }
                }
                if payment_methods.contains(&ConventionPaymentMethod::Stripe) {
                    button {
                        id: "checkout-card",
                        class: "checkout-payment-button checkout-card",
                        disabled: unavailable,
                        onclick: move |_| {
                            if *busy.peek() {
                                return;
                            }
                            let window = browser::PaymentWindow::reserve();
                            busy.set(true);
                            submitting.set(true);
                            let slug = slug.clone();
                            let snapshot = snapshot.clone();
                            let quote = quote.clone();
                            let customer = card_customer.clone();
                            spawn(async move {
                                begin(
                                        slug,
                                        snapshot,
                                        quote,
                                        customer,
                                        CheckoutProvider::Stripe,
                                        SubmissionState {
                                            busy,
                                            error,
                                            attempt,
                                            delayed_recovery,
                                            submitting,
                                            provider_open,
                                        },
                                        window,
                                    )
                                    .await;
                            });
                        },
                        span { class: "checkout-payment-marks",
                            PaymentMarks { provider: CheckoutProvider::Stripe }
                        }
                        span { "{card_label}" }
                    }
                }
            }
            p { class: "hint", "Wallet availability depends on your device and the payment gateway." }
            if *busy.read() {
                LoadingMessage { message: "Preparing your secure payment…" }
            }
            ErrorMessage { message: error.read().clone().or_else(|| saved_state.and_then(Result::err)) }
        }
    }
}

#[component]
pub fn CheckoutReturnPage(slug: String, query: String) -> Element {
    rsx! {
        CheckoutStatusPage { slug, query, cancelled: false }
    }
}

#[component]
pub fn CheckoutCancelPage(slug: String, query: String) -> Element {
    rsx! {
        CheckoutStatusPage { slug, query, cancelled: true }
    }
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
    rsx! {
        ConventionFrame { back_to,
            match state {
                None => rsx! {
                    LoadingMessage { message: "Opening your checkout…" }
                },
                Some(Err(message)) => rsx! {
                    Heading { title: "Payment recovery" }
                    ErrorMessage { message: Some(message) }
                },
                Some(Ok(attempt)) => rsx! {
                    PaymentSession { key: "{attempt.operation_id}", attempt, cancelled }
                },
            }
        }
    }
}

#[component]
fn RecoverSavedPayment(attempt: Attempt, delayed: bool, #[props(default)] manual: bool) -> Element {
    let saved = attempt.clone();
    let navigation = use_resource(move || {
        let attempt = saved.clone();
        async move {
            if browser::ignored(&attempt).await? {
                return Ok(true);
            }
            if manual {
                return Ok(false);
            }
            if delayed && !browser::delay(api::backoff_seconds(1)).await {
                return Err("Automatic payment recovery is unavailable in this browser. Keep your payment receipt and contact the shop before paying again.".to_string());
            }
            browser::navigate(&payment_route(&attempt)).await?;
            Ok(false)
        }
    });
    match navigation.read().clone() {
        Some(Ok(true)) => rsx! {
            IgnoredCheckout { attempt }
        },
        Some(Ok(false)) if manual => rsx! {
            RecoveryActions { attempt }
        },
        Some(Err(message)) => rsx! {
            ErrorMessage { message: Some(message) }
        },
        _ => rsx! {
            LoadingMessage { message: "Opening your original payment…" }
        },
    }
}

#[component]
fn RecoveryActions(attempt: Attempt) -> Element {
    let mut error = use_signal(|| None::<String>);
    let mut opening = use_signal(|| false);
    let original = attempt.clone();
    rsx! {
        p {
            "Your cart is saved. You can continue checking this payment or cancel this checkout and start a new order."
        }
        button {
            class: "primary",
            disabled: *opening.read(),
            onclick: move |_| {
                opening.set(true);
                let attempt = original.clone();
                spawn(async move {
                    if let Err(message) = browser::navigate(&payment_route(&attempt)).await {
                        error.set(Some(message));
                        opening.set(false);
                    }
                });
            },
            "Continue checking"
        }
        button {
            class: "button-link ghost",
            disabled: *opening.read(),
            onclick: move |_| {
                opening.set(true);
                let attempt = attempt.clone();
                spawn(async move {
                    if let Err(message) = cancel_checkout(&attempt).await {
                        error.set(Some(message));
                        opening.set(false);
                    }
                });
            },
            "Cancel and return to cart"
        }
        ErrorMessage { message: error.read().clone() }
    }
}

#[component]
fn IgnoredCheckout(attempt: Attempt) -> Element {
    let mut error = use_signal(|| None::<String>);
    let mut opening = use_signal(|| false);
    rsx! {
        Heading { title: "Checkout cancelled" }
        p {
            "This checkout is no longer being checked in this browser. Your cart is saved and you can start a new order."
        }
        button {
            class: "button-link primary",
            disabled: *opening.read(),
            onclick: move |_| {
                opening.set(true);
                let attempt = attempt.clone();
                spawn(async move {
                    if let Err(message) = cancel_checkout(&attempt).await {
                        error.set(Some(message));
                        opening.set(false);
                    }
                });
            },
            "Return to cart"
        }
        ErrorMessage { message: error.read().clone() }
    }
}

#[component]
fn PaymentSession(
    attempt: Attempt,
    cancelled: bool,
    #[props(default)] awaiting_provider: bool,
) -> Element {
    let observation = use_signal(Observation::default);
    let mut coordination = use_signal(|| None::<Result<browser::CheckingState, String>>);
    let mut provider_open = use_signal(|| awaiting_provider);
    let finished = use_signal(|| false);
    let mut cancellation_requested = use_signal(|| false);
    let mut leaving = use_signal(|| false);
    let mut exit_error = use_signal(|| None::<String>);
    let mut watching = use_checking_session(&attempt, coordination);
    let checking_attempt = attempt.clone();
    let mut polling = use_resource(move || {
        let owner = !*finished.read()
            && !*cancellation_requested.read()
            && matches!(
                *coordination.read(),
                Some(Ok(browser::CheckingState::Owner))
            );
        let attempt = checking_attempt.clone();
        let awaiting_provider = *provider_open.read();
        async move {
            if owner {
                poll_payment(
                    &attempt,
                    awaiting_provider,
                    observation,
                    coordination,
                    finished,
                )
                .await;
            }
        }
    });
    use_effect(move || {
        if *finished.read() {
            if coordination.peek().is_some() {
                coordination.set(None);
            }
            watching.cancel();
        } else if matches!(
            *coordination.read(),
            Some(Ok(browser::CheckingState::Ignored))
        ) {
            watching.cancel();
        }
    });
    let checking_state = coordination.read().clone();
    if matches!(checking_state, Some(Ok(browser::CheckingState::Ignored))) {
        return rsx! {
            IgnoredCheckout { attempt }
        };
    }
    let state = observation.read().clone();
    let passive = matches!(checking_state, Some(Ok(browser::CheckingState::Passive)));
    let checking = *cancellation_requested.read()
        || (state.error.is_none()
            && state.response.as_ref().is_none_or(|r| {
                matches!(
                    r.status,
                    CheckoutStatus::Processing | CheckoutStatus::RequiresAction
                )
            }));
    let cancel_attempt = attempt.clone();
    rsx! {
        section { class: "panel checkout-status", aria_live: "polite",
            if cancelled {
                p { class: "portal-notice",
                    "You left the payment page. That does not confirm cancellation. An already approved payment may still complete."
                }
            }
            if *cancellation_requested.read() {
                Heading { title: "Return to your cart" }
                p { "Finish cancelling this checkout to start a new order." }
            } else if let Some(Err(message)) = checking_state {
                ErrorMessage { message: Some(message) }
            } else if passive {
                Heading { title: "Payment is being checked in another tab" }
                p {
                    "Keep the original payment tab open. This page will take over checking automatically if that tab closes."
                }
            } else if let Some(response) = state.response {
                if *provider_open.read() && response.status == CheckoutStatus::RequiresAction {
                    ProcessingPayment {}
                } else {
                    StatusContent {
                        response: response.clone(),
                        provider: attempt.provider,
                        attempt: Some(attempt.clone()),
                        on_open: move |()| provider_open.set(true),
                    }
                }
                if response.status == CheckoutStatus::Unpaid {
                    Link {
                        to: Route::CartPage {
                            slug: attempt.slug.clone(),
                            checkout_id: attempt.snapshot.checkout_id.clone(),
                        },
                        class: "button-link ghost",
                        "Return to cart"
                    }
                }
            } else if state.error.is_none() {
                ProcessingPayment {}
            }
            if checking {
                button {
                    class: "button-link ghost",
                    disabled: *leaving.read(),
                    onclick: move |_| {
                        if *leaving.peek() {
                            return;
                        }
                        leaving.set(true);
                        cancellation_requested.set(true);
                        coordination.set(None);
                        exit_error.set(None);
                        polling.cancel();
                        watching.cancel();
                        let attempt = cancel_attempt.clone();
                        spawn(async move {
                            let result = cancel_checkout(&attempt).await;
                            if let Err(message) = result {
                                exit_error.set(Some(message));
                                leaving.set(false);
                            }
                        });
                    },
                    "Cancel and return to cart"
                }
            }
            ErrorMessage { message: state.error.or_else(|| exit_error.read().clone()) }
        }
    }
}

fn use_checking_session(attempt: &Attempt, mut state: Signal<Coordination>) -> Resource<()> {
    let watched = attempt.clone();
    use_resource(move || {
        let attempt = watched.clone();
        async move {
            let result = async {
                let mut session = browser::CheckingSession::new(&attempt)?;
                loop {
                    let next = session.next().await?;
                    state.set(Some(Ok(next)));
                    if next == browser::CheckingState::Ignored {
                        break;
                    }
                }
                Ok::<_, String>(())
            }
            .await;
            if let Err(message) = result {
                state.set(Some(Err(message)));
            }
        }
    })
}

async fn poll_payment(
    attempt: &Attempt,
    awaiting_provider: bool,
    mut observation: Signal<Observation>,
    mut coordination: Signal<Coordination>,
    mut finished: Signal<bool>,
) {
    let mut failures = 0_u32;
    if awaiting_provider && !browser::delay(api::backoff_seconds(0)).await {
        observation.set(Observation {response:None,error:Some("Automatic payment recovery is unavailable in this browser. Keep your payment receipt and contact the shop before paying again.".into())});
        finished.set(true);
        return;
    }
    loop {
        let retrying = match inspect(attempt).await {
            Ok(response) => {
                let pending = matches!(
                    response.status,
                    CheckoutStatus::Processing | CheckoutStatus::RequiresAction
                );
                let terminal = response.status != CheckoutStatus::RequiresAction;
                observation.set(Observation {
                    response: Some(response),
                    error: None,
                });
                if !pending {
                    if terminal {
                        finished.set(true);
                    }
                    break;
                }
                failures = 0;
                false
            }
            Err(error) if error.ignored => {
                coordination.set(Some(Ok(browser::CheckingState::Ignored)));
                break;
            }
            Err(error) if error.retryable => true,
            Err(error) => {
                observation.set(Observation {
                    response: None,
                    error: Some(error.message),
                });
                finished.set(true);
                break;
            }
        };
        if retrying {
            failures = failures.saturating_add(1);
        }
        if !browser::delay(api::backoff_seconds(failures)).await {
            observation.set(Observation {response:None,error:Some("Automatic payment recovery is unavailable in this browser. Keep your payment receipt and contact the shop before paying again.".into())});
            finished.set(true);
            break;
        }
    }
}

#[component]
fn StatusContent(
    response: CheckoutResponse,
    provider: CheckoutProvider,
    on_open: Option<EventHandler<()>>,
    attempt: Option<Attempt>,
) -> Element {
    let mut opening = use_signal(|| false);
    let mut navigation_error = use_signal(|| None::<String>);
    match response.status {
        CheckoutStatus::Completed => {
            let order = response
                .order
                .expect("completed response checked before rendering");
            rsx! {
                div { class: "checkout-confirmation",
                    img {
                        class: "checkout-site-logo",
                        src: ui::SITE_LOGO,
                        alt: "Enzumeii",
                    }
                    Heading { title: format!("Congratulations! Your order {} is confirmed.", order.name) }
                    p {
                        "Check your email for your order confirmation."
                        br {}
                        "Show that email at the convention to pick up your order."
                    }
                }
            }
        }
        CheckoutStatus::RequiresAction => rsx! {
            Heading { title: "Continue your payment" }
            p {
                "Your original checkout is ready. Continue with the same payment to finish your preorder."
            }
            button {
                class: "primary",
                disabled: *opening.read(),
                onclick: move |_| {
                    if *opening.peek() {
                        return;
                    }
                    let mut window = browser::PaymentWindow::reserve();
                    opening.set(true);
                    navigation_error.set(None);
                    let response = response.clone();
                    let attempt = attempt.clone();
                    spawn(async move {
                        let result = async {
                            window.ready().await?;
                            if let Some(attempt) = attempt.as_ref()
                                && browser::ignored(attempt).await?
                            {
                                return Err(
                                    "This checkout was cancelled. Return to your cart to start a new order."
                                        .into(),
                                );
                            }
                            hosted_redirect(&response, provider, window, attempt.as_ref()).await
                        }
                            .await;
                        match result {
                            Ok(()) => {
                                if let Some(on_open) = on_open {
                                    on_open.call(());
                                }
                            }
                            Err(message) => navigation_error.set(Some(message)),
                        }
                        opening.set(false);
                    });
                },
                "Continue payment"
            }
            ErrorMessage { message: navigation_error.read().clone() }
        },
        CheckoutStatus::Processing => rsx! {
            ProcessingPayment {}
        },
        CheckoutStatus::Unpaid => rsx! {
            Heading { title: "Payment could not be completed" }
            p {
                "This checkout is unpaid. Your cart is saved. Return to your cart to review your order."
            }
        },
        CheckoutStatus::ReconciliationRequired => rsx! {
            Heading { title: "Your order needs a payment check" }
            p {
                "Please do not pay again. Keep your payment receipt and contact the shop so we can confirm your preorder. Your cart is saved."
            }
        },
    }
}

#[component]
fn ProcessingPayment() -> Element {
    rsx! {
        div { class: "checkout-processing", role: "status",
            img {
                class: "checkout-site-logo",
                src: ui::SITE_LOGO,
                alt: "Enzumeii",
            }
            Heading { title: "Your payment is being checked" }
            div { class: "checkout-progress", aria_hidden: "true" }
            p {
                "We are confirming your payment and preparing your order. Please do not pay again. Your cart is saved."
            }
        }
    }
}

#[component]
fn PaymentMarks(provider: CheckoutProvider) -> Element {
    match provider {
        CheckoutProvider::Paypal => rsx! {
            svg {
                class: "checkout-paypal-mark",
                width: "30",
                height: "32",
                view_box: "0 0 32 34",
                "aria-hidden": "true",
                "focusable": "false",
                path {
                    fill: "#003087",
                    d: "M8 1h12c8 0 10 5 8 11-2 6-6 8-12 8h-4l-2 12H3z",
                }
                path {
                    fill: "#009cde",
                    d: "M13 8h10c7 0 9 4 7 10-2 6-6 8-11 8h-3l-1 7H8z",
                    opacity: ".95",
                }
                path {
                    fill: "#012169",
                    d: "M13 8h7c4 0 7 1 8 3-1 6-6 9-12 9h-5z",
                }
            }
        },
        CheckoutProvider::Stripe => rsx! {
            span { class: "checkout-wallet-marks", aria_hidden: "true",
                svg {
                    width: "30",
                    height: "24",
                    view_box: "0 0 30 24",
                    fill: "none",
                    stroke: "currentColor",
                    stroke_width: "1.8",
                    "focusable": "false",
                    rect {
                        x: "1",
                        y: "2",
                        width: "28",
                        height: "20",
                        rx: "3",
                    }
                    path { d: "M1 8h28M6 16h6" }
                }
                svg {
                    width: "46",
                    height: "24",
                    view_box: "0 0 46 24",
                    "focusable": "false",
                    path {
                        fill: "#4285f4",
                        d: "M14 12.2c0-.5 0-1-.1-1.5H7.5v2.6h3.6a3 3 0 0 1-1.4 2v2.1h2.2c1.3-1.2 2.1-3 2.1-5.2",
                    }
                    path {
                        fill: "#34a853",
                        d: "M7.5 19c1.9 0 3.5-.6 4.6-1.6l-2.2-2.1c-.6.4-1.3.6-2.4.6-1.8 0-3.3-1.2-3.9-2.8H1.3v2.2A7 7 0 0 0 7.5 19",
                    }
                    path {
                        fill: "#fbbc05",
                        d: "M3.6 13.1a4.3 4.3 0 0 1 0-2.6V8.3H1.3a7 7 0 0 0 0 7z",
                    }
                    path {
                        fill: "#ea4335",
                        d: "M7.5 7.6c1 0 1.8.3 2.5 1l2-2a7 7 0 0 0-10.7 1.7l2.3 2.2a4.1 4.1 0 0 1 3.9-2.9",
                    }
                    text {
                        x: "17",
                        y: "17",
                        fill: "currentColor",
                        font_size: "12",
                        font_family: "Arial, sans-serif",
                        "Pay"
                    }
                }
                svg {
                    width: "49",
                    height: "24",
                    view_box: "0 0 49 24",
                    fill: "currentColor",
                    "focusable": "false",
                    path { d: "M12 5c.8-1 1.3-2.1 1.2-3.3-1.2 0-2.6.8-3.4 1.7-.7.8-1.4 2.1-1.2 3.3 1.3.1 2.6-.6 3.4-1.7M15.4 12.6c0-2 1.7-3 1.8-3.1-1-1.5-2.6-1.7-3.2-1.7-1.4-.1-2.7.8-3.4.8-.7 0-1.8-.8-3-.8-1.6 0-3 1-3.8 2.3-1.6 2.8-.4 6.8 1.2 9.1.7 1.1 1.6 2.3 2.8 2.2 1.2 0 1.6-.7 3.1-.7s1.9.7 3.1.7c1.3 0 2.1-1.1 2.8-2.2.9-1.2 1.2-2.4 1.2-2.5-.1 0-2.6-1-2.6-4.1" }
                    text {
                        x: "21",
                        y: "18",
                        font_size: "14",
                        font_family: "Arial, sans-serif",
                        "Pay"
                    }
                }
            }
        },
    }
}

async fn begin(
    slug: String,
    snapshot: Cart,
    quote: Option<ConventionQuote>,
    customer: CustomerDetails,
    provider: CheckoutProvider,
    state: SubmissionState,
    mut window: browser::PaymentWindow,
) {
    let SubmissionState {
        mut busy,
        mut error,
        mut attempt,
        mut delayed_recovery,
        mut submitting,
        mut provider_open,
    } = state;
    error.set(None);
    let result = async {
        let quote = quote.ok_or("Review your current cart total before paying.")?;
        let request = model::request(
            &snapshot,
            &quote,
            customer.email.trim(),
            &customer.note,
            provider,
        )?;
        window.ready().await?;
        let (ownership, started) = browser::start(&slug, &snapshot, &request, &quote.total).await?;
        let mut saved = started.attempt.clone();
        let fresh = started.fresh;
        let result = advance(started).await;
        let status = result
            .as_ref()
            .map_or_else(|e| e.disposition, |r| Some(status_name(r.status)));
        ownership.finish(status).await?;
        if browser::ignored(&saved).await? {
            attempt.set(Some(saved));
            provider_open.set(true);
            submitting.set(false);
            return Ok(true);
        }
        if status == Some("rejected") {
            saved.state = "rejected".into();
        }
        attempt.set(Some(saved.clone()));
        let response = match result {
            Err(error) if error.retryable => {
                delayed_recovery.set(true);
                return Ok(false);
            }
            result => result.map_err(|error| error.message)?,
        };
        if fresh && response.status == CheckoutStatus::RequiresAction {
            hosted_redirect(&response, saved.provider, window, Some(&saved)).await?;
            provider_open.set(true);
            submitting.set(false);
        } else {
            drop(window);
            browser::navigate(&payment_route(&saved)).await?;
        }
        Ok::<_, String>(true)
    }
    .await;
    match result {
        Ok(true) => {}
        Ok(false) => submitting.set(false),
        Err(message) => {
            error.set(Some(message));
            busy.set(false);
            submitting.set(false);
        }
    }
}

async fn inspect(attempt: &Attempt) -> Result<CheckoutResponse, AttemptError> {
    let (ownership, started) = match browser::recover(attempt).await {
        Ok(owned) => owned,
        Err(message) => {
            ensure_not_ignored(attempt).await?;
            return Err(recovery_error(message));
        }
    };
    let result = advance(started).await;
    let status = result
        .as_ref()
        .map_or_else(|e| e.disposition, |r| Some(status_name(r.status)));
    let checkpoint = ownership.finish(status).await;
    ensure_not_ignored(attempt).await?;
    checkpoint.map_err(recovery_error)?;
    if status == Some("completed") {
        invalidate_completed_cart(attempt);
    }
    result
}

async fn advance(started: browser::Started) -> Result<CheckoutResponse, AttemptError> {
    let attempt = &started.attempt;
    ensure_not_ignored(attempt).await?;
    let response = if started.fresh {
        create_saved(&started).await
    } else {
        match api::orders::verify(&attempt.operation_id, &attempt.secret).await {
            Err(error) if matches!(error.status, 401 | 404) && started.payload.is_some() => {
                create_saved(&started).await
            }
            result => result.map_err(|error| request_error(&error, false)),
        }
    }?;
    model::checked_response(attempt, &response).map_err(|message| AttemptError {
        message,
        disposition: None,
        retryable: false,
        ignored: false,
    })?;
    Ok(response)
}

async fn create_saved(started: &browser::Started) -> Result<CheckoutResponse, AttemptError> {
    let payload = started.payload.as_deref().ok_or_else(|| {
        recovery_error("The original checkout details are no longer available in this tab.".into())
    })?;
    let request: domain::dto::checkout::CreateCheckoutRequest = serde_json::from_str(payload)
        .map_err(|_| recovery_error("The original checkout details cannot be restored.".into()))?;
    if !model::saved_request_matches(&started.attempt, &request) {
        return Err(recovery_error(
            "The original checkout details do not match.".into(),
        ));
    }
    ensure_not_ignored(&started.attempt).await?;
    api::orders::create(&request, &started.attempt.secret)
        .await
        .map_err(|error| request_error(&error, started.fresh))
}

async fn ensure_not_ignored(attempt: &Attempt) -> Result<(), AttemptError> {
    if browser::ignored(attempt).await.map_err(recovery_error)? {
        let mut error = recovery_error("This checkout was cancelled.".into());
        error.ignored = true;
        return Err(error);
    }
    Ok(())
}

fn recovery_error(message: String) -> AttemptError {
    AttemptError {
        message,
        disposition: None,
        retryable: false,
        ignored: false,
    }
}

fn request_error(error: &api::ApiError, fresh: bool) -> AttemptError {
    let expired = matches!(error.status, 401 | 403);
    let rejected = matches!(error.status, 400 | 422);
    let retryable = error.is_transient();
    AttemptError {
        message: if expired {
            "This browser can no longer verify this checkout. Keep your payment receipt and contact the shop before paying again."
        } else if rejected && fresh {
            "Payment could not be started. Review your confirmation email and current cart details before paying."
        } else if retryable {
            "Payment is taking longer than expected. We are checking your original payment automatically."
        } else {
            "Payment recovery could not continue. Keep your payment receipt and contact the shop before paying again."
        }.into(),
        disposition: if expired { Some("expired") } else if rejected && fresh { Some("rejected") } else { None },
        retryable,
        ignored: false,
    }
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
    window: browser::PaymentWindow,
    attempt: Option<&Attempt>,
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
    if let Some(attempt) = attempt
        && browser::ignored(attempt).await?
    {
        return Err(
            "This checkout was cancelled. Return to your cart to start a new order.".into(),
        );
    }
    if window
        .handoff(response, provider, test_origin.as_deref())
        .await?
    {
        Ok(())
    } else {
        Err("This payment does not require a redirect.".into())
    }
}

async fn cancel_checkout(attempt: &Attempt) -> Result<(), String> {
    let cart = browser::abandon(attempt).await?;
    browser::navigate(&cart_route(&attempt.slug, &cart)).await
}

fn cart_route(slug: &str, cart: &Cart) -> String {
    format!("/{slug}/cart/{}", cart.checkout_id)
}

fn payment_route(attempt: &Attempt) -> String {
    format!(
        "/{}/checkout/return?checkout_id={}",
        attempt.slug, attempt.operation_id
    )
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

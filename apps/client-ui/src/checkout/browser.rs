pub use crate::api::delay;

use super::model::{self, Attempt};
use crate::cart::Cart;
use dioxus::{document::Eval, prelude::*};
use domain::dto::checkout::{CheckoutProvider, CheckoutResponse, CreateCheckoutRequest};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Deserialize)]
struct Reply {
    ok: bool,
    #[serde(default)]
    value: Value,
    error: Option<String>,
}

#[derive(Deserialize)]
pub struct Started {
    pub attempt: Attempt,
    pub payload: Option<String>,
    pub fresh: bool,
}

pub struct PaymentWindow {
    eval: Eval,
    ready: bool,
    released: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CheckingState {
    Passive,
    Owner,
    Ignored,
}

pub struct CheckingSession {
    eval: Eval,
    released: bool,
}

pub struct Ownership {
    eval: Eval,
    released: bool,
}

pub async fn read(
    slug: &str,
    convention_id: Option<&str>,
    id: Option<&str>,
) -> Result<Option<Attempt>, String> {
    let mut eval =
        evaluate(&json!({"action":"read","slug":slug,"convention_id":convention_id,"id":id}))?;
    receive(&mut eval).await
}

pub async fn start(
    slug: &str,
    snapshot: &Cart,
    request: &CreateCheckoutRequest,
    total: &str,
) -> Result<(Ownership, Started), String> {
    let payload = serde_json::to_string(request).map_err(|_| "Unable to prepare this checkout.")?;
    own(&json!({"action":"start","slug":slug,"convention_id":snapshot.convention_id,"snapshot":snapshot,"provider":request.provider,"payload":payload,"currency":request.expected_currency,"total":total})).await
}

pub async fn recover(attempt: &Attempt) -> Result<(Ownership, Started), String> {
    own(&json!({"action":"recover","slug":attempt.slug,"convention_id":attempt.snapshot.convention_id,"id":attempt.operation_id})).await
}

pub async fn ignored(attempt: &Attempt) -> Result<bool, String> {
    attempt_action(attempt, "ignored").await
}

pub async fn abandon(attempt: &Attempt) -> Result<Cart, String> {
    attempt_action(attempt, "abandon").await
}

pub async fn navigate(url: &str) -> Result<(), String> {
    let url = serde_json::to_string(url).map_err(|_| "Unable to open the payment page.")?;
    document::eval(&format!(
        "window.location.assign({url}); dioxus.send(true);"
    ))
    .recv::<bool>()
    .await
    .map_err(|_| {
        "Unable to open the payment page in this browser. Keep any payment receipt and contact the shop if the payment page remains unavailable.".to_string()
    })?;
    Ok(())
}

impl PaymentWindow {
    pub fn reserve() -> Self {
        Self {
            eval: document::eval(include_str!("payment-window.js")),
            ready: false,
            released: false,
        }
    }

    pub async fn ready(&mut self) -> Result<(), String> {
        if !self.ready {
            receive::<()>(&mut self.eval).await?;
            self.ready = true;
        }
        Ok(())
    }

    pub async fn handoff(
        mut self,
        response: &CheckoutResponse,
        provider: CheckoutProvider,
        test_origin: Option<&str>,
    ) -> Result<bool, String> {
        let Some(url) = model::redirect(response, provider, test_origin)? else {
            return Ok(false);
        };
        self.ready().await?;
        self.eval
            .send(json!({"action":"open","url":url,"checkout_id":response.checkout_id}))
            .map_err(|_| "Unable to open the payment window. Resume this payment to try again.")?;
        self.released = true;
        receive::<()>(&mut self.eval).await?;
        Ok(true)
    }
}

impl Drop for PaymentWindow {
    fn drop(&mut self) {
        if !self.released {
            let _ = self.eval.send(json!({"action":"abort"}));
        }
    }
}

impl CheckingSession {
    pub fn new(attempt: &Attempt) -> Result<Self, String> {
        let input = serde_json::to_string(&json!({"id":attempt.operation_id,"slug":attempt.slug}))
            .map_err(|_| "Unable to prepare payment checking.")?;
        Ok(Self {
            eval: document::eval(&include_str!("checking-session.js").replace("__INPUT__", &input)),
            released: false,
        })
    }

    pub async fn next(&mut self) -> Result<CheckingState, String> {
        let state = receive(&mut self.eval).await;
        if matches!(state, Ok(CheckingState::Ignored)) {
            self.released = true;
        }
        state
    }
}

impl Drop for CheckingSession {
    fn drop(&mut self) {
        if !self.released {
            let _ = self.eval.send(json!({"action":"release"}));
        }
    }
}

impl Ownership {
    pub async fn finish(mut self, status: Option<&str>) -> Result<bool, String> {
        self.eval
            .send(json!({"status":status}))
            .map_err(|_| "Unable to save checkout recovery information.")?;
        self.released = true;
        receive(&mut self.eval).await
    }
}

impl Drop for Ownership {
    fn drop(&mut self) {
        if !self.released {
            let _ = self.eval.send(json!({"status":null}));
        }
    }
}

async fn attempt_action<T: DeserializeOwned>(attempt: &Attempt, action: &str) -> Result<T, String> {
    let mut eval =
        evaluate(&json!({"action":action,"slug":attempt.slug,"id":attempt.operation_id}))?;
    receive(&mut eval).await
}

async fn own(input: &Value) -> Result<(Ownership, Started), String> {
    let mut ownership = Ownership {
        eval: evaluate(input)?,
        released: false,
    };
    let started = receive(&mut ownership.eval).await?;
    Ok((ownership, started))
}

fn evaluate(input: &Value) -> Result<Eval, String> {
    let input = serde_json::to_string(input).map_err(|_| "Unable to prepare payment recovery.")?;
    Ok(document::eval(
        &include_str!("bridge.js").replace("__INPUT__", &input),
    ))
}

async fn receive<T: DeserializeOwned>(eval: &mut Eval) -> Result<T, String> {
    let reply: Reply = eval
        .recv()
        .await
        .map_err(|_| "Unable to access payment recovery information in this browser.")?;
    if !reply.ok {
        return Err(reply
            .error
            .unwrap_or_else(|| "Payment recovery is unavailable.".into()));
    }
    serde_json::from_value(reply.value)
        .map_err(|_| "Payment recovery information is unavailable.".into())
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "../tests/checkout/browser.rs"]
mod tests;

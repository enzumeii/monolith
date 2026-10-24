use super::model::Attempt;
use crate::cart::Cart;
use dioxus::{document::Eval, prelude::*};
use domain::dto::checkout::CreateCheckoutRequest;
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

pub async fn navigate(url: &str) -> Result<(), String> {
    let url = serde_json::to_string(url).map_err(|_| "Unable to open the payment page.")?;
    document::eval(&format!(
        "window.location.assign({url}); dioxus.send(true);"
    ))
    .recv::<bool>()
    .await
    .map_err(|_| {
        "Unable to open the payment page. Use Continue payment to try again.".to_string()
    })?;
    Ok(())
}

pub async fn delay() -> bool {
    document::eval("await new Promise(resolve => setTimeout(resolve, 5000)); dioxus.send(true);")
        .recv::<bool>()
        .await
        .unwrap_or(false)
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

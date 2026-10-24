use super::Cart;
use dioxus::prelude::*;
use serde::Deserialize;

#[derive(Clone, PartialEq)]
pub struct Restored {
    pub cart: Cart,
    pub notice: Option<String>,
}

#[derive(Deserialize)]
struct StorageRead {
    raw: Option<String>,
    checkout_id: String,
    blocked: bool,
}

pub async fn visitor() -> Result<(String, Option<String>), String> {
    document::eval(include_str!("visitor.js"))
        .recv()
        .await
        .map_err(|_| "Unable to determine your local date. Please reload.".into())
}

pub async fn restore(convention_id: &str) -> Result<Restored, String> {
    let key = serde_json::to_string(&storage_key(convention_id))
        .map_err(|_| "Unable to open your cart")?;
    let script = include_str!("restore.js").replace("__STORAGE_KEY__", &key);
    let read: StorageRead = document::eval(&script)
        .recv()
        .await
        .map_err(|_| "Unable to initialize your cart. Please reload.")?;
    let restored = read
        .raw
        .as_deref()
        .and_then(|raw| Cart::restore(raw, convention_id));
    let notice = if read.blocked {
        Some(
            "Browser storage is unavailable. Your cart will remain in this tab while it is open."
                .into(),
        )
    } else if read.raw.is_some() && restored.is_none() {
        Some("The saved cart could not be restored. A new cart has been opened.".into())
    } else {
        None
    };
    if !super::model::valid_checkout_id(&read.checkout_id) {
        return Err(
            "Unable to generate a cart identifier. Please reload in a secure browser.".into(),
        );
    }
    Ok(Restored {
        cart: restored.unwrap_or_else(|| Cart::new(convention_id.to_owned(), read.checkout_id)),
        notice,
    })
}

pub async fn save(cart: &Cart) -> Result<(), String> {
    let key = serde_json::to_string(&storage_key(&cart.convention_id))
        .map_err(|_| "Unable to save your cart")?;
    let value = serde_json::to_string(cart)
        .and_then(|value| serde_json::to_string(&value))
        .map_err(|_| "Unable to save your cart")?;
    let script = format!(
        "try {{ localStorage.setItem({key}, {value}); dioxus.send(true); }} catch (_) {{ dioxus.send(false); }}"
    );
    if document::eval(&script)
        .recv::<bool>()
        .await
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err("Browser storage is unavailable. Changes remain in this tab while it is open.".into())
    }
}

fn storage_key(convention_id: &str) -> String {
    format!("enzumeii.cart.v1.{convention_id}")
}

#[cfg(test)]
#[path = "../tests/cart/browser.rs"]
mod tests;

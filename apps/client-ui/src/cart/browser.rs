use super::Cart;
use dioxus::{document::Eval, prelude::*};
use serde::Deserialize;

#[derive(Clone, PartialEq)]
pub struct Restored {
    pub cart: Cart,
    pub notice: Option<String>,
}

#[derive(Deserialize)]
struct SaveRead {
    raw: Option<String>,
}

struct Saving {
    eval: Eval,
    released: bool,
}

#[derive(Deserialize)]
struct StorageRead {
    raw: Option<String>,
    checkout_id: String,
    blocked: bool,
}

pub async fn visitor(
    settings: &crate::config::VisitorConfig,
) -> Result<(String, Option<String>), String> {
    let url = serde_json::to_string(&settings.country_lookup_url)
        .map_err(|_| "Unable to configure country detection")?;
    let script = include_str!("visitor.js")
        .replace("__LOOKUP_URL__", &url)
        .replace("__TIMEOUT_MS__", &settings.timeout_ms.to_string());
    document::eval(&script).recv().await.map_err(|_| {
        "Unable to determine your local date. Check that JavaScript is enabled in your browser."
            .into()
    })
}

pub async fn restore(convention_id: &str) -> Result<Restored, String> {
    let key = serde_json::to_string(&storage_key(convention_id))
        .map_err(|_| "Unable to open your cart")?;
    let script = include_str!("restore.js").replace("__STORAGE_KEY__", &key);
    let read: StorageRead = document::eval(&script)
        .recv()
        .await
        .map_err(|_| "Unable to initialize your cart. Check that JavaScript and browser storage are available.")?;
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
            "Unable to generate a cart identifier. Open the shop in a secure browser.".into(),
        );
    }
    Ok(Restored {
        cart: restored.unwrap_or_else(|| Cart::new(convention_id.to_owned(), read.checkout_id)),
        notice,
    })
}

pub async fn save(cart: &Cart) -> Result<Cart, String> {
    let key = serde_json::to_string(&storage_key(&cart.convention_id)).map_err(|_| save_error())?;
    let convention = serde_json::to_string(&cart.convention_id).map_err(|_| save_error())?;
    let value = serde_json::to_string(cart)
        .and_then(|value| serde_json::to_string(&value))
        .map_err(|_| save_error())?;
    let script = include_str!("save.js")
        .replace("__STORAGE_KEY__", &key)
        .replace("__CONVENTION_ID__", &convention)
        .replace("__CART_JSON__", &value);
    let mut saving = Saving {
        eval: document::eval(&script),
        released: false,
    };
    let read: SaveRead = saving.eval.recv().await.map_err(|_| save_error())?;
    let accepted = read
        .raw
        .as_deref()
        .and_then(|raw| Cart::restore(raw, &cart.convention_id))
        .filter(|saved| saved.checkout_id != cart.checkout_id)
        .unwrap_or_else(|| cart.clone());
    saving
        .eval
        .send(accepted.checkout_id == cart.checkout_id)
        .map_err(|_| save_error())?;
    saving.released = true;
    if saving.eval.recv::<bool>().await.unwrap_or(false) {
        Ok(accepted)
    } else {
        Err(save_error())
    }
}

impl Drop for Saving {
    fn drop(&mut self) {
        if !self.released {
            let _ = self.eval.send(false);
        }
    }
}

fn save_error() -> String {
    "Browser storage is unavailable. Changes remain in this tab while it is open.".into()
}

fn storage_key(convention_id: &str) -> String {
    format!("enzumeii.cart.v1.{convention_id}")
}

#[cfg(test)]
#[path = "../tests/cart/browser.rs"]
mod tests;

use config::ConfigError;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct CheckoutSettings {
    pub frontend_origin: String,
    pub return_path_template: String,
    pub cancel_path_template: String,
    pub shopify_location_id: String,
    pub reservation_secs: u64,
    pub guest_recovery_secs: u64,
    pub stripe_livemode: bool,
    pub paypal_merchant_id: Option<String>,
    pub stripe_webhook_secret: Option<String>,
    pub paypal_webhook_id: Option<String>,
}

impl CheckoutSettings {
    pub fn validate(&self, insecure_local: bool) -> Result<(), ConfigError> {
        super::validate_browser_origin(&self.frontend_origin, insecure_local)?;
        if self.reservation_secs < 1800
            || self.reservation_secs > 86400
            || self.guest_recovery_secs < self.reservation_secs
            || self.guest_recovery_secs > 31 * 86400
            || crate::validations::inventories::parse_location_id(&self.shopify_location_id)
                .and_then(|location| location.number("Location").ok())
                .is_none()
        {
            return Err(ConfigError::Message("Invalid checkout settings".into()));
        }
        for template in [&self.return_path_template, &self.cancel_path_template] {
            if !template.starts_with("/{convention_slug}/")
                || !template.contains("{checkout_id}")
                || template.contains(['#', '\\', '@'])
                || template.contains("..")
                || template.chars().any(char::is_whitespace)
            {
                return Err(ConfigError::Message(
                    "Invalid checkout return path template".into(),
                ));
            }
            let path = template
                .replace("{convention_slug}", "convention")
                .replace("{checkout_id}", "00000000-0000-0000-0000-000000000000");
            if path.contains(['{', '}']) {
                return Err(ConfigError::Message(
                    "Invalid checkout return path template".into(),
                ));
            }
        }
        Ok(())
    }
}

impl std::fmt::Debug for CheckoutSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckoutSettings")
            .field("frontend_origin", &self.frontend_origin)
            .field("reservation_secs", &self.reservation_secs)
            .field("guest_recovery_secs", &self.guest_recovery_secs)
            .field("credentials", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "../tests/config/checkout.rs"]
mod tests;

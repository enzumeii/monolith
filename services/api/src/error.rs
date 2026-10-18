use axum::{
    Json,
    http::{StatusCode, header::WWW_AUTHENTICATE},
    response::{IntoResponse, Response},
};
use domain::dto::ErrorResponse;
use std::{borrow::Cow, sync::Arc};
use thiserror::Error;

#[derive(Error)]
#[error("{message}")]
pub struct AppError {
    status: StatusCode,
    message: Cow<'static, str>,
    #[source]
    cause: Option<anyhow::Error>,
    origin: Option<&'static str>,
    contexts: Vec<&'static str>,
    bearer_challenge: bool,
}

impl AppError {
    #[must_use]
    pub fn new(status: StatusCode, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            status,
            message: message.into(),
            cause: None,
            origin: None,
            contexts: Vec::new(),
            bearer_challenge: false,
        }
    }

    #[must_use]
    pub fn with_source(mut self, source: impl Into<anyhow::Error>) -> Self {
        self.cause = Some(source.into());
        self
    }

    #[must_use]
    pub fn with_bearer_challenge(mut self) -> Self {
        self.bearer_challenge = true;
        self
    }

    #[must_use]
    pub fn context(mut self, operation: &'static str) -> Self {
        self.contexts.push(operation);
        self
    }

    #[must_use]
    pub fn at(mut self, function: &'static str) -> Self {
        self.origin.get_or_insert(function);
        self
    }

    fn origin(&self) -> Option<&'static str> {
        self.origin
    }

    fn operation(&self, fallback: &'static str) -> &'static str {
        self.contexts.first().copied().unwrap_or(fallback)
    }
}

impl std::fmt::Debug for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppError")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        emit("HTTP request", &self);
        let mut response = (
            self.status,
            Json(ErrorResponse {
                error: self.message.into_owned(),
            }),
        )
            .into_response();
        if self.bearer_challenge {
            response.headers_mut().insert(
                WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer"),
            );
        }
        response
    }
}

/// Emit a failure once when it will not be converted to an HTTP response.
pub fn log_background_error(operation: &'static str, error: &AppError) {
    emit(operation, error);
}

impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").with_source(error)
    }
}

impl From<config::ConfigError> for AppError {
    fn from(error: config::ConfigError) -> Self {
        Self::from(anyhow::Error::new(error))
    }
}

impl From<sqlx::migrate::MigrateError> for AppError {
    fn from(error: sqlx::migrate::MigrateError) -> Self {
        Self::from(anyhow::Error::new(error))
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::from(anyhow::Error::new(error))
    }
}

impl From<argon2::password_hash::Error> for AppError {
    fn from(error: argon2::password_hash::Error) -> Self {
        Self::from(anyhow::Error::new(error))
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(error: jsonwebtoken::errors::Error) -> Self {
        Self::from(anyhow::Error::new(error))
    }
}

impl From<crate::client::ClientError> for AppError {
    fn from(error: crate::client::ClientError) -> Self {
        let origin = match &error {
            crate::client::ClientError::Origin { origin, .. }
            | crate::client::ClientError::Transport { origin, .. }
            | crate::client::ClientError::Decode { origin, .. } => *origin,
            _ => "api::client",
        };
        Self::from(anyhow::Error::new(error)).at(origin)
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref db) = error
            && db.is_unique_violation()
            && db.constraint() == Some("admin_username_key")
        {
            return Self::new(StatusCode::CONFLICT, "Username already exists").with_source(error);
        }
        Self::from(anyhow::Error::new(error))
    }
}

pub(crate) fn client_timeout(error: &crate::client::ClientError) -> bool {
    match error {
        crate::client::ClientError::Origin { source, .. } => client_timeout(source),
        crate::client::ClientError::Transport { source, .. } => source.is_timeout(),
        _ => false,
    }
}

fn emit(operation: &'static str, error: &AppError) {
    let operation = error.operation(operation);
    let origin = error.origin().unwrap_or("unknown");
    let status = error.status.as_u16();
    if error.status.is_client_error() {
        tracing::warn!(operation, origin, status, "API request rejected");
    } else {
        tracing::error!(operation, origin, status, "API operation failed");
    }
    tracing::debug!(operation, origin, status, error = ?Diagnostic(error), "API failure diagnostics");
}

struct Diagnostic<'a>(&'a AppError);

impl std::fmt::Debug for Diagnostic<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut diagnostic = f.debug_struct("AppError");
        diagnostic.field("contexts", &self.0.contexts);
        if let Some(cause) = &self.0.cause {
            if let Some(source) = cause.chain().find_map(client_cause) {
                let contexts = cause
                    .chain()
                    .take_while(|source| client_cause(*source).is_none())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                diagnostic.field("cause_contexts", &contexts);
                if matches!(
                    self.0.status,
                    StatusCode::BAD_GATEWAY
                        | StatusCode::SERVICE_UNAVAILABLE
                        | StatusCode::GATEWAY_TIMEOUT
                ) {
                    diagnostic.field("cause", &ClientDiagnostic(source));
                } else {
                    diagnostic.field("cause", source);
                }
            } else {
                let source: &(dyn std::error::Error + Send + Sync) = cause.as_ref();
                diagnostic.field("cause", &source);
            }
        } else {
            diagnostic.field("message", &self.0.message);
        }
        diagnostic.finish()
    }
}

fn client_cause<'a>(
    source: &'a (dyn std::error::Error + 'static),
) -> Option<&'a crate::client::ClientError> {
    source
        .downcast_ref::<crate::client::ClientError>()
        .or_else(|| {
            source
                .downcast_ref::<Arc<crate::client::ClientError>>()
                .map(Arc::as_ref)
        })
}

#[cfg(test)]
#[path = "tests/error.rs"]
mod tests;

struct ClientDiagnostic<'a>(&'a crate::client::ClientError);
impl std::fmt::Debug for ClientDiagnostic<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::client::ClientError;
        match self.0 {
            ClientError::Origin { source, .. } => Self(source).fmt(f),
            ClientError::Transport { source, .. } => std::fmt::Debug::fmt(source, f),
            ClientError::Decode { source, .. } => std::fmt::Debug::fmt(source, f),
            ClientError::Graphql { errors, .. } => f
                .debug_list()
                .entries(errors.iter().map(|error| &error.message))
                .finish(),
            ClientError::Mutation { errors } => f
                .debug_list()
                .entries(errors.iter().map(|error| &error.message))
                .finish(),
            error => std::fmt::Debug::fmt(error, f),
        }
    }
}

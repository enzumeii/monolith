use crate::client::ClientError;
use axum::{
    Json,
    http::{StatusCode, header::WWW_AUTHENTICATE},
    response::{IntoResponse, Response},
};
use domain::dto::ErrorResponse;
use std::{borrow::Cow, panic::Location, sync::Arc};
use thiserror::Error;

#[derive(Error)]
#[error("{message}")]
pub struct AppError {
    status: StatusCode,
    message: Cow<'static, str>,
    #[source]
    cause: Option<anyhow::Error>,
    location: &'static Location<'static>,
    contexts: Vec<&'static str>,
    bearer_challenge: bool,
}

impl AppError {
    #[must_use]
    #[track_caller]
    pub fn new(status: StatusCode, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            status,
            message: message.into(),
            cause: None,
            location: Location::caller(),
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

    pub(crate) fn location(&self) -> &'static Location<'static> {
        self.location
    }

    pub(crate) fn find_source<T: std::error::Error + 'static>(&self) -> Option<&T> {
        self.cause.as_ref()?.chain().find_map(|source| {
            source.downcast_ref::<T>().or_else(|| {
                let mut source = std::error::Error::source(client_cause(source)?);
                while let Some(current) = source {
                    if let Some(found) = current.downcast_ref::<T>() {
                        return Some(found);
                    }
                    source = current.source();
                }
                None
            })
        })
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
    #[track_caller]
    fn from(error: anyhow::Error) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").with_source(error)
    }
}

impl From<config::ConfigError> for AppError {
    #[track_caller]
    fn from(error: config::ConfigError) -> Self {
        Self::from(anyhow::Error::new(error))
    }
}

impl From<sqlx::migrate::MigrateError> for AppError {
    #[track_caller]
    fn from(error: sqlx::migrate::MigrateError) -> Self {
        Self::from(anyhow::Error::new(error))
    }
}

impl From<std::io::Error> for AppError {
    #[track_caller]
    fn from(error: std::io::Error) -> Self {
        Self::from(anyhow::Error::new(error))
    }
}

impl From<argon2::password_hash::Error> for AppError {
    #[track_caller]
    fn from(error: argon2::password_hash::Error) -> Self {
        Self::from(anyhow::Error::new(error))
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    #[track_caller]
    fn from(error: jsonwebtoken::errors::Error) -> Self {
        Self::from(anyhow::Error::new(error))
    }
}

impl From<crate::client::ClientError> for AppError {
    #[track_caller]
    fn from(error: crate::client::ClientError) -> Self {
        Self::from(anyhow::Error::new(error))
    }
}

impl From<sqlx::Error> for AppError {
    #[track_caller]
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

#[track_caller]
pub(crate) fn upstream(error: ClientError) -> AppError {
    let status = if client_timeout(&error) {
        StatusCode::GATEWAY_TIMEOUT
    } else {
        StatusCode::BAD_GATEWAY
    };
    AppError::new(status, "Upstream request failed").with_source(error)
}

pub(crate) fn client_timeout(error: &crate::client::ClientError) -> bool {
    match error.kind() {
        crate::client::ClientErrorKind::Transport { source } => source.is_timeout(),
        crate::client::ClientErrorKind::Upstream { timed_out, .. } => *timed_out,
        _ => false,
    }
}

fn emit(operation: &'static str, error: &AppError) {
    let operation = error.operation(operation);
    let origin = error.location();
    let client_origin = client_origin(error).map(ToString::to_string);
    let status = error.status.as_u16();
    if error.status.is_client_error() {
        tracing::warn!(operation, %origin, client_origin, status, "API request rejected");
    } else {
        tracing::error!(operation, %origin, client_origin, status, "API operation failed");
    }
    tracing::debug!(operation, %origin, client_origin, status, error = ?Diagnostic(error), "API failure diagnostics");
}

fn client_origin(error: &AppError) -> Option<&'static Location<'static>> {
    let source = error.cause.as_ref()?.chain().find_map(client_cause)?;
    Some(source.location())
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

struct ClientDiagnostic<'a>(&'a crate::client::ClientError);
impl std::fmt::Debug for ClientDiagnostic<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::client::ClientErrorKind;
        match self.0.kind() {
            ClientErrorKind::Transport { source } => std::fmt::Debug::fmt(source, f),
            ClientErrorKind::Decode { source } => std::fmt::Debug::fmt(source, f),
            ClientErrorKind::Graphql { errors, .. } => f
                .debug_list()
                .entries(errors.iter().map(|error| &error.message))
                .finish(),
            ClientErrorKind::Mutation { errors } => f
                .debug_list()
                .entries(errors.iter().map(|error| &error.message))
                .finish(),
            _ => std::fmt::Debug::fmt(self.0, f),
        }
    }
}

#[cfg(test)]
#[path = "tests/error.rs"]
mod tests;

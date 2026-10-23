use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub deployed: bool,
}

impl Command {
    pub fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Self, AppError> {
        let arguments: Vec<_> = arguments.into_iter().collect();
        match arguments.as_slice() {
            [] => Ok(Self { deployed: false }),
            [command] if command == "server" => Ok(Self { deployed: true }),
            _ => Err(AppError::new(
                axum::http::StatusCode::BAD_REQUEST,
                "Expected api or api server",
            )),
        }
    }
}

#[cfg(test)]
#[path = "tests/startup.rs"]
mod tests;

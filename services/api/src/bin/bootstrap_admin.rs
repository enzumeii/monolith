use api::{
    config::Settings,
    error::{self, AppError},
    repositories,
};
use domain::dto::{AdminRole, CreateAdminRequest};
use std::io::{IsTerminal, Read};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    match run().await {
        Ok(id) => {
            println!("Created admin {id}");
            std::process::ExitCode::SUCCESS
        }
        Err(failure) => {
            error::log_background_error("bootstrap admin", &failure);
            std::process::ExitCode::FAILURE
        }
    }
}
async fn run() -> Result<i64, AppError> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 || std::io::stdin().is_terminal() {
        return Err(AppError::BadRequest(
            "Usage: bootstrap-admin USERNAME ADMIN|SUPER_ADMIN; provide password on stdin".into(),
        ));
    }
    let role = match args[1].as_str() {
        "ADMIN" => AdminRole::Admin,
        "SUPER_ADMIN" => AdminRole::SuperAdmin,
        _ => return Err(AppError::BadRequest("Invalid bootstrap role".into())),
    };
    let mut password = String::new();
    std::io::stdin().take(515).read_to_string(&mut password)?;
    if password.len() > 514 {
        return Err(AppError::BadRequest(
            "Bootstrap password input is too long".into(),
        ));
    }
    if password.ends_with("\r\n") {
        password.truncate(password.len() - 2);
    } else if password.ends_with('\n') {
        password.pop();
    }
    let settings = Settings::new()?;
    let request = CreateAdminRequest {
        username: args[0].clone(),
        password,
        role,
    };
    api::validate_create(&request)?;
    let limiter = api::PasswordLimiter::new(settings.password_concurrency);
    let pool = repositories::create_pool(&settings).await?;
    let account = api::bootstrap_admin(&pool, &limiter, request).await?;
    Ok(account.id)
}

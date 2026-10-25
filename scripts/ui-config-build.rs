use std::{
    env,
    error::Error,
    path::{Path, PathBuf},
    process::Command,
};

fn main() -> Result<(), Box<dyn Error>> {
    let app = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let root = app.join("../..");
    let environment = match env::var("RUN_ENV") {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => {
            if env::var("PROFILE").as_deref() == Ok("release") {
                "prod".into()
            } else {
                "local".into()
            }
        }
        Err(error) => return Err(error.into()),
    };
    if environment.is_empty()
        || !environment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(
            "RUN_ENV must be a nonempty environment name containing letters, digits, '_' or '-'"
                .into(),
        );
    }
    let explicit = env::var_os("UI_CONFIG_OVERRIDE");
    let overlay = explicit.as_ref().map_or_else(
        || app.join(format!("application.{environment}.toml")),
        |file| app.join(file),
    );
    let output_directory = app.join(format!("target/public-config/{environment}"));
    let baseline_only = explicit.is_none() && environment == "prod" && !overlay.exists();
    let composer = root.join("scripts/compose-vercel-ui-config.mjs");
    for file in [
        app.join("application.toml"),
        overlay.clone(),
        composer.clone(),
    ] {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    println!("cargo:rerun-if-env-changed=RUN_ENV");
    println!("cargo:rerun-if-env-changed=UI_CONFIG_OVERRIDE");
    let source = if baseline_only {
        app.join("application.toml")
    } else {
        overlay.clone()
    }
    .canonicalize()?;
    println!(
        "cargo:rustc-env=PUBLIC_CONFIG_OVERLAY_SOURCE={}",
        source.display()
    );
    let result = Command::new("node")
        .arg(composer)
        .arg(app.join("application.toml"))
        .arg(if baseline_only {
            PathBuf::from("-")
        } else {
            overlay
        })
        .arg(output_directory)
        .arg("--fingerprint")
        .output()?;
    if !result.status.success() {
        eprint!("{}", String::from_utf8_lossy(&result.stderr));
        return Err("Unable to compose public UI configuration; run root `just setup` and check the selected application.<env>.toml".into());
    }
    let generated = String::from_utf8(result.stdout)?;
    let relative = Path::new(generated.trim()).strip_prefix(&app)?;
    let asset = format!("/{}", relative.to_string_lossy().replace('\\', "/"));
    println!("cargo:rustc-env=PUBLIC_CONFIG_ASSET={asset}");
    Ok(())
}

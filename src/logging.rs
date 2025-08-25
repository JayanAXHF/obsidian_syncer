use color_eyre::Result;
use directories::ProjectDirs;
use std::env;
use std::path::PathBuf;
use std::sync::LazyLock;
use tracing_error::ErrorLayer;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub static PROJECT_NAME: LazyLock<String> =
    LazyLock::new(|| env!("CARGO_CRATE_NAME").to_uppercase().to_string());

pub static LOG_ENV: LazyLock<String> =
    LazyLock::new(|| format!("{}_LOG_LEVEL", PROJECT_NAME.clone()));
pub static LOG_FILE: LazyLock<String> = LazyLock::new(|| format!("{}.log", env!("CARGO_PKG_NAME")));
pub static DATA_FOLDER: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    env::var(format!("{}_DATA", PROJECT_NAME.clone()))
        .ok()
        .map(PathBuf::from)
});

pub fn init() -> Result<()> {
    let directory = get_data_dir();
    std::fs::create_dir_all(directory.clone())?;
    let log_path = directory.join(LOG_FILE.clone());
    println!("Logging to {}", log_path.display());
    let log_file = std::fs::File::create(log_path)?;
    let env_filter = EnvFilter::builder().with_default_directive(tracing::Level::DEBUG.into());
    // If the `RUST_LOG` environment variable is set, use that as the default, otherwise use the
    // value of the `LOG_ENV` environment variable. If the `LOG_ENV` environment variable contains
    // errors, then this will return an error.
    let env_filter = env_filter
        .try_from_env()
        .or_else(|_| env_filter.with_env_var(LOG_ENV.clone()).from_env())?;
    let file_subscriber = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_writer(log_file)
        .with_target(false)
        .with_ansi(false)
        .with_filter(env_filter);
    tracing_subscriber::registry()
        .with(file_subscriber)
        .with(ErrorLayer::default())
        .try_init()?;
    Ok(())
}
pub fn get_data_dir() -> PathBuf {
    let directory = if let Some(s) = DATA_FOLDER.clone() {
        s
    } else if let Some(proj_dirs) = project_directory() {
        proj_dirs.data_local_dir().to_path_buf()
    } else {
        PathBuf::from(".").join(".data")
    };
    directory
}

fn project_directory() -> Option<ProjectDirs> {
    ProjectDirs::from("com", "jayanaxhf", env!("CARGO_PKG_NAME"))
}


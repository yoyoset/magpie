//! File logging under `%APPDATA%\magpie\logs\`, rotated daily.

use magpie_config::paths::Paths;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

/// Keeps the background log writer alive; dropping it flushes and stops logging.
pub struct LogGuard(#[allow(dead_code)] WorkerGuard);

pub fn init(paths: &Paths) -> anyhow::Result<LogGuard> {
    let dir = paths.root().join("logs");
    std::fs::create_dir_all(&dir)?;
    let (writer, guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::daily(&dir, "magpie.log"));
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("MAGPIE_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(writer)
        .with_ansi(false)
        .init();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "magpie starting");
    Ok(LogGuard(guard))
}

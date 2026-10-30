//! Log file (always) plus stderr (warnings, or everything with `-v`). Rotated at 20 MB.

use anyhow::Result;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

use diskgc_core::paths;

const ROTATE_AT: u64 = 20 * 1024 * 1024;

pub fn init(verbose: bool, daemon: bool) -> Result<()> {
    let path = paths::log_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::metadata(&path)
        .map(|m| m.len() > ROTATE_AT)
        .unwrap_or(false)
    {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_target(false)
        .with_writer(std::sync::Mutex::new(file))
        .with_filter(LevelFilter::INFO);
    let stderr_level = if verbose || daemon {
        LevelFilter::INFO
    } else {
        LevelFilter::WARN
    };
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .without_time()
        .with_writer(std::io::stderr)
        .with_filter(stderr_level);
    tracing_subscriber::registry()
        .with(file_layer)
        .with(stderr_layer)
        .init();
    Ok(())
}

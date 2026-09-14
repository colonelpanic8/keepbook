use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use keepbook::config::{default_config_path, ResolvedConfig};
use keepbook::storage::{JsonFileStorage, Storage};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

mod activation;
mod daemon;
mod parse;
mod tray;

use daemon::Daemon;

const CLI_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (git commit ",
    env!("GIT_COMMIT_HASH"),
    ")"
);

fn parse_duration_arg(s: &str) -> Result<Duration, String> {
    keepbook::duration::parse_duration(s).map_err(|e| e.to_string())
}

fn parse_nonzero_duration_arg(s: &str) -> Result<Duration, String> {
    let duration = parse_duration_arg(s)?;
    if duration.is_zero() {
        return Err("duration must be greater than 0s".to_string());
    }
    Ok(duration)
}

#[derive(Parser, Debug)]
#[command(name = "keepbook-sync-daemon")]
#[command(version = CLI_VERSION)]
#[command(about = "Long-running keepbook sync daemon with tray controls")]
struct Cli {
    /// Path to keepbook config file.
    #[arg(short, long, default_value_os_t = default_config_path())]
    config: PathBuf,

    /// Base sync interval (e.g. "30m", "1h", "1d").
    #[arg(long, default_value = "30m", value_parser = parse_duration_arg)]
    interval: Duration,

    /// Add random jitter in the range [-jitter, +jitter] to each interval.
    #[arg(long, default_value = "0s", value_parser = parse_duration_arg)]
    jitter: Duration,

    /// How often to refresh tray content from local data files as a fallback safety net.
    #[arg(long, default_value = "30s", value_parser = parse_nonzero_duration_arg)]
    refresh_interval: Duration,

    /// Override balance staleness threshold for `sync --if-stale` behavior.
    #[arg(long, value_name = "DURATION", value_parser = parse_duration_arg)]
    balance_staleness: Option<Duration>,

    /// Override price staleness threshold used by price refresh.
    #[arg(long, value_name = "DURATION", value_parser = parse_duration_arg)]
    price_staleness: Option<Duration>,

    /// Maximum number of recent portfolio history rows shown in tray menu (overrides `[tray].history_points`).
    #[arg(long, value_name = "COUNT")]
    history_points: Option<usize>,

    /// Skip the immediate startup sync cycle.
    #[arg(long)]
    no_sync_on_start: bool,

    /// Disable periodic price refresh.
    #[arg(long)]
    no_sync_prices: bool,

    /// Disable periodic symlink rebuild.
    #[arg(long)]
    no_sync_symlinks: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new(
                "info,chromiumoxide=warn,chromiumoxide::conn=off,chromiumoxide::handler=off",
            )
        }))
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(true)
                .with_level(true)
                .json(),
        )
        .init();

    let cli = Cli::parse();

    let mut config = ResolvedConfig::load_or_default(&cli.config)
        .with_context(|| format!("Failed to load keepbook config: {}", cli.config.display()))?;

    if let Some(balance_staleness) = cli.balance_staleness {
        config.refresh.balance_staleness = balance_staleness;
    }

    if let Some(price_staleness) = cli.price_staleness {
        config.refresh.price_staleness = price_staleness;
    }

    let storage_impl = JsonFileStorage::new(&config.data_dir);
    let storage: Arc<dyn Storage> = Arc::new(storage_impl.clone());
    let history_points = cli.history_points.unwrap_or(config.tray.history_points);
    let spending_windows_days = config.tray.spending_windows_days.clone();
    let transaction_count = config.tray.transaction_count;

    let daemon = Daemon {
        storage,
        symlink_storage: storage_impl,
        config,
        interval: cli.interval,
        jitter: cli.jitter,
        refresh_interval: cli.refresh_interval,
        sync_on_start: !cli.no_sync_on_start,
        sync_prices: !cli.no_sync_prices,
        sync_symlinks: !cli.no_sync_symlinks,
        history_points,
        spending_windows_days,
        transaction_count,
    };

    daemon.run().await
}

#[cfg(test)]
#[path = "../../../tests/unit/bin/keepbook_sync_daemon/main_tests.rs"]
mod main_tests;

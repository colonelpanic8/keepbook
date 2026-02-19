use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDate};
use clap::Parser;
use keepbook::app;
use keepbook::config::{default_config_path, ResolvedConfig};
use keepbook::format::format_base_currency_display;
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::TransactionSyncMode;
use ksni::menu::*;
use ksni::MenuItem;
use ksni::TrayMethods;
use rand::Rng;
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::sync::mpsc::{self, UnboundedSender};
use tracing::{info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

// --- Embedded icon PNGs (compiled into the binary) ---
const ICON_32_PNG: &[u8] = include_bytes!("../../assets/keepbook-icon-32.png");
const ICON_48_PNG: &[u8] = include_bytes!("../../assets/keepbook-icon-48.png");
const ICON_64_PNG: &[u8] = include_bytes!("../../assets/keepbook-icon-64.png");

const OVERLAY_SYNC_32: &[u8] = include_bytes!("../../assets/overlay-sync-32.png");
const OVERLAY_SYNC_48: &[u8] = include_bytes!("../../assets/overlay-sync-48.png");
const OVERLAY_SYNC_64: &[u8] = include_bytes!("../../assets/overlay-sync-64.png");

const OVERLAY_ERROR_32: &[u8] = include_bytes!("../../assets/overlay-error-32.png");
const OVERLAY_ERROR_48: &[u8] = include_bytes!("../../assets/overlay-error-48.png");
const OVERLAY_ERROR_64: &[u8] = include_bytes!("../../assets/overlay-error-64.png");

fn png_to_argb32(png_data: &[u8]) -> ksni::Icon {
    let img = image::load_from_memory_with_format(png_data, image::ImageFormat::Png)
        .expect("embedded PNG is valid")
        .into_rgba8();
    let width = img.width() as i32;
    let height = img.height() as i32;
    // Convert RGBA → ARGB (network byte order for StatusNotifierItem)
    let data: Vec<u8> = img
        .pixels()
        .flat_map(|p| [p[3], p[0], p[1], p[2]])
        .collect();
    ksni::Icon {
        width,
        height,
        data,
    }
}

fn load_icon_set(png_32: &[u8], png_48: &[u8], png_64: &[u8]) -> Vec<ksni::Icon> {
    vec![
        png_to_argb32(png_32),
        png_to_argb32(png_48),
        png_to_argb32(png_64),
    ]
}

struct OverlayIcons {
    sync: Vec<ksni::Icon>,
    error: Vec<ksni::Icon>,
}

impl std::fmt::Debug for OverlayIcons {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayIcons").finish_non_exhaustive()
    }
}

fn parse_duration_arg(s: &str) -> Result<Duration, String> {
    keepbook::duration::parse_duration(s).map_err(|e| e.to_string())
}

#[derive(Parser, Debug)]
#[command(name = "keepbook-sync-daemon")]
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

    /// Override balance staleness threshold for `sync --if-stale` behavior.
    #[arg(long, value_name = "DURATION", value_parser = parse_duration_arg)]
    balance_staleness: Option<Duration>,

    /// Override price staleness threshold used by price refresh.
    #[arg(long, value_name = "DURATION", value_parser = parse_duration_arg)]
    price_staleness: Option<Duration>,

    /// Number of recent portfolio history points shown in tray menu.
    #[arg(long, default_value_t = 8)]
    history_points: usize,

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum DaemonStatus {
    Idle,
    Syncing,
    Error(String),
}

#[derive(Debug, Clone)]
enum DaemonCommand {
    SyncNow,
    Quit,
}

#[derive(Debug, Clone)]
struct KeepbookTrayState {
    status: DaemonStatus,
    last_cycle: Option<DateTime<Local>>,
    next_cycle: Option<DateTime<Local>>,
    last_summary: String,
    history_lines: Vec<String>,
    spending_lines: Vec<String>,
}

impl Default for KeepbookTrayState {
    fn default() -> Self {
        Self {
            status: DaemonStatus::Idle,
            last_cycle: None,
            next_cycle: None,
            last_summary: "No sync cycle has run yet".to_string(),
            history_lines: vec!["No portfolio history loaded".to_string()],
            spending_lines: vec!["Spending metrics not loaded".to_string()],
        }
    }
}

impl KeepbookTrayState {
    fn status_text(&self) -> String {
        match &self.status {
            DaemonStatus::Idle => "Idle".to_string(),
            DaemonStatus::Syncing => "Syncing...".to_string(),
            DaemonStatus::Error(msg) => format!("Error: {msg}"),
        }
    }

    fn last_cycle_text(&self) -> String {
        match self.last_cycle {
            Some(ts) => format!("Last cycle: {}", ts.format("%Y-%m-%d %H:%M:%S %Z")),
            None => "Last cycle: never".to_string(),
        }
    }

    fn next_cycle_text(&self) -> String {
        match self.next_cycle {
            Some(ts) => format!("Next cycle: {}", ts.format("%Y-%m-%d %H:%M:%S %Z")),
            None => "Next cycle: unscheduled".to_string(),
        }
    }
}

struct KeepbookTray {
    state: KeepbookTrayState,
    cmd_tx: UnboundedSender<DaemonCommand>,
    icons: Vec<ksni::Icon>,
    overlays: OverlayIcons,
    icon_generation: u64,
}

impl KeepbookTray {
    fn new(state: KeepbookTrayState, cmd_tx: UnboundedSender<DaemonCommand>) -> Self {
        Self {
            state,
            cmd_tx,
            icons: load_icon_set(ICON_32_PNG, ICON_48_PNG, ICON_64_PNG),
            overlays: OverlayIcons {
                sync: load_icon_set(OVERLAY_SYNC_32, OVERLAY_SYNC_48, OVERLAY_SYNC_64),
                error: load_icon_set(OVERLAY_ERROR_32, OVERLAY_ERROR_48, OVERLAY_ERROR_64),
            },
            icon_generation: 0,
        }
    }

    fn bump_icon_generation(&mut self) {
        self.icon_generation = self.icon_generation.wrapping_add(1);
    }

    fn overlay_pixmaps_for_status(&self) -> Vec<ksni::Icon> {
        let base = match &self.state.status {
            DaemonStatus::Idle => return self.generation_only_pixmap(),
            DaemonStatus::Syncing => &self.overlays.sync,
            DaemonStatus::Error(_) => &self.overlays.error,
        };
        let mut icons = base.clone();
        icons.push(self.generation_pixel());
        icons
    }

    fn generation_pixel(&self) -> ksni::Icon {
        let gen = self.icon_generation;
        let r = (gen & 0xFF) as u8;
        let g = ((gen >> 8) & 0xFF) as u8;
        let b = ((gen >> 16) & 0xFF) as u8;
        ksni::Icon {
            width: 1,
            height: 1,
            data: vec![0, r, g, b],
        }
    }

    fn generation_only_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.generation_pixel()]
    }
}

impl ksni::Tray for KeepbookTray {
    const MENU_ON_ACTIVATE: bool = true;

    fn id(&self) -> String {
        "keepbook-sync-daemon".to_string()
    }

    fn title(&self) -> String {
        "keepbook sync daemon".to_string()
    }

    fn icon_name(&self) -> String {
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }

    fn overlay_icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.overlay_pixmaps_for_status()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "keepbook sync daemon".to_string(),
            description: format!(
                "{}\n{}\n{}\n{}",
                self.state.status_text(),
                self.state.last_cycle_text(),
                self.state.next_cycle_text(),
                self.state.last_summary,
            ),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let history_menu: Vec<MenuItem<Self>> = if self.state.history_lines.is_empty() {
            vec![StandardItem {
                label: "No portfolio history available".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into()]
        } else {
            self.state
                .history_lines
                .iter()
                .map(|line| {
                    StandardItem {
                        label: line.clone(),
                        enabled: false,
                        ..Default::default()
                    }
                    .into()
                })
                .collect()
        };

        let spending_menu: Vec<MenuItem<Self>> = if self.state.spending_lines.is_empty() {
            vec![StandardItem {
                label: "No spending metrics available".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into()]
        } else {
            self.state
                .spending_lines
                .iter()
                .map(|line| {
                    StandardItem {
                        label: line.clone(),
                        enabled: false,
                        ..Default::default()
                    }
                    .into()
                })
                .collect()
        };

        let mut items = vec![
            StandardItem {
                label: "keepbook sync daemon".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: format!("Status: {}", self.state.status_text()),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.state.last_cycle_text(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.state.next_cycle_text(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: self.state.last_summary.clone(),
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            SubMenu {
                label: "Recent Portfolio History".to_string(),
                icon_name: "view-calendar-timeline".to_string(),
                submenu: history_menu,
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Recent Spending".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into(),
        ];

        items.extend(spending_menu);
        items.extend([
            MenuItem::Separator,
            StandardItem {
                label: "Sync Now".to_string(),
                icon_name: "view-refresh".to_string(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.cmd_tx.send(DaemonCommand::SyncNow);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".to_string(),
                icon_name: "application-exit".to_string(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.cmd_tx.send(DaemonCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]);

        items
    }
}

#[derive(Debug, Default)]
struct SyncCounts {
    total: usize,
    synced: usize,
    skipped_manual: usize,
    skipped_not_stale: usize,
    failed: usize,
}

fn parse_sync_counts(value: &serde_json::Value) -> SyncCounts {
    let mut counts = SyncCounts {
        total: value
            .get("total")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as usize,
        ..Default::default()
    };

    let Some(results) = value.get("results").and_then(serde_json::Value::as_array) else {
        return counts;
    };

    for result in results {
        let success = result
            .get("success")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        if !success {
            counts.failed += 1;
            continue;
        }

        let skipped = result
            .get("skipped")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        if !skipped {
            counts.synced += 1;
            continue;
        }

        match result
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
        {
            "manual" => counts.skipped_manual += 1,
            "not stale" => counts.skipped_not_stale += 1,
            _ => counts.skipped_not_stale += 1,
        }
    }

    counts
}

fn parse_price_counts(value: &serde_json::Value) -> (usize, usize, usize) {
    let result = value.get("result").cloned().unwrap_or_default();
    let fetched = result
        .get("fetched")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let skipped = result
        .get("skipped")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let failed = result
        .get("failed_count")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    (fetched, skipped, failed)
}

fn parse_symlink_counts(value: &serde_json::Value) -> (usize, usize) {
    let connection_symlinks = value
        .get("connection_symlinks_created")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    let account_symlinks = value
        .get("account_symlinks_created")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    (connection_symlinks, account_symlinks)
}

fn compute_next_delay(interval: Duration, jitter: Duration) -> Duration {
    if jitter.is_zero() {
        return interval;
    }

    let base_ms = interval.as_millis().min(u128::from(u64::MAX)) as i128;
    let jitter_ms = jitter.as_millis().min(u128::from(u64::MAX)) as i128;
    let offset = rand::thread_rng().gen_range(-jitter_ms..=jitter_ms);

    let min_ms = 1_000_i128;
    let max_ms = i128::from(u64::MAX);
    let delay_ms = (base_ms + offset).clamp(min_ms, max_ms) as u64;
    Duration::from_millis(delay_ms)
}

fn local_now_plus(duration: Duration) -> DateTime<Local> {
    match chrono::Duration::from_std(duration) {
        Ok(d) => Local::now() + d,
        Err(_) => Local::now() + chrono::Duration::days(365 * 100),
    }
}

fn default_currency_symbol(currency: &str) -> Option<&'static str> {
    match currency.to_ascii_uppercase().as_str() {
        "USD" => Some("$"),
        "EUR" => Some("€"),
        "GBP" => Some("£"),
        "JPY" => Some("¥"),
        _ => None,
    }
}

fn format_tray_currency(
    value: &str,
    currency: &str,
    display: &keepbook::config::DisplayConfig,
) -> String {
    // The tray is a UI surface: default to sane currency rounding even when the
    // global config doesn't set `display.currency_decimals`.
    let dp = display.currency_decimals.or(Some(2));
    let symbol = display
        .currency_symbol
        .as_deref()
        .or_else(|| default_currency_symbol(currency));
    match Decimal::from_str(value) {
        Ok(d) => {
            let formatted = format_base_currency_display(
                d,
                dp,
                display.currency_grouping,
                symbol,
                display.currency_fixed_decimals,
            );
            if symbol.is_some() {
                formatted
            } else {
                format!("{formatted} {currency}")
            }
        }
        Err(_) => value.to_string(),
    }
}

async fn apply_tray_state(
    tray_handle: &mut Option<ksni::Handle<KeepbookTray>>,
    state: &KeepbookTrayState,
) {
    let Some(handle) = tray_handle.as_ref() else {
        return;
    };

    let new_state = state.clone();
    let update_result = handle
        .update(move |tray: &mut KeepbookTray| {
            tray.state = new_state;
            tray.bump_icon_generation();
        })
        .await;

    if update_result.is_none() {
        warn!("Tray update failed; disabling tray updates for this process");
        *tray_handle = None;
    }
}

struct Daemon {
    storage: Arc<dyn Storage>,
    symlink_storage: JsonFileStorage,
    config: ResolvedConfig,
    interval: Duration,
    jitter: Duration,
    sync_on_start: bool,
    sync_prices: bool,
    sync_symlinks: bool,
    history_points: usize,
}

impl Daemon {
    fn last_n_days_range(days: u32) -> (NaiveDate, NaiveDate) {
        let end = Local::now().date_naive();
        let start = end - chrono::Duration::days(days.saturating_sub(1) as i64);
        (start, end)
    }

    async fn spending_line_for_days(&self, days: u32) -> String {
        let (start, end) = Self::last_n_days_range(days);
        let opts = app::SpendingReportOptions {
            currency: None,
            start: Some(start.format("%Y-%m-%d").to_string()),
            end: Some(end.format("%Y-%m-%d").to_string()),
            period: "range".to_string(),
            tz: None,
            week_start: None,
            bucket: None,
            account: None,
            connection: None,
            status: "posted".to_string(),
            direction: "outflow".to_string(),
            group_by: "none".to_string(),
            top: None,
            lookback_days: 7,
            include_noncurrency: false,
            include_empty: false,
        };

        match app::spending_report(self.storage.as_ref(), &self.config, opts).await {
            Ok(report) => {
                let value =
                    format_tray_currency(&report.total, &report.currency, &self.config.display);
                let tx_label = if report.transaction_count == 1 {
                    "txn"
                } else {
                    "txns"
                };
                format!(
                    "Last {days}d: {} ({} {})",
                    value, report.transaction_count, tx_label
                )
            }
            Err(err) => {
                warn!(
                    window_days = days,
                    error = %err,
                    "unable to refresh tray spending metrics"
                );
                format!("Last {days}d: unavailable")
            }
        }
    }

    async fn refresh_spending_lines(&self, state: &mut KeepbookTrayState) {
        let mut lines = Vec::with_capacity(3);
        for days in [7_u32, 30, 90] {
            lines.push(self.spending_line_for_days(days).await);
        }
        state.spending_lines = lines;
    }

    async fn refresh_history_lines(&self, state: &mut KeepbookTrayState) {
        match app::portfolio_history(
            self.storage.clone(),
            &self.config,
            None,
            None,
            None,
            "daily".to_string(),
            true,
        )
        .await
        {
            Ok(history) => {
                let mut lines: Vec<String> = history
                    .points
                    .iter()
                    .rev()
                    .take(self.history_points)
                    .map(|point| {
                        let value = format_tray_currency(
                            &point.total_value,
                            &history.currency,
                            &self.config.display,
                        );
                        format!("{}: {}", point.date, value)
                    })
                    .collect();

                if lines.is_empty() {
                    lines.push("No portfolio history available".to_string());
                }

                state.history_lines = lines;
            }
            Err(err) => {
                state.history_lines = vec![format!("History unavailable: {err}")];
            }
        }
    }

    async fn run_cycle(
        &self,
        reason: &str,
        state: &mut KeepbookTrayState,
        tray_handle: &mut Option<ksni::Handle<KeepbookTray>>,
    ) {
        state.status = DaemonStatus::Syncing;
        state.last_summary = format!("Running sync cycle ({reason})");
        apply_tray_state(tray_handle, state).await;

        let cycle_result = async {
            app::run_preflight(
                &self.config,
                app::PreflightOptions {
                    merge_origin_master: self.config.git.merge_master_before_command,
                },
            )?;

            let sync_json =
                app::sync_all_if_stale(self.storage.clone(), &self.config, TransactionSyncMode::Auto)
                    .await?;
            let sync_counts = parse_sync_counts(&sync_json);

            let (prices_fetched, prices_skipped, prices_failed) = if self.sync_prices {
                let prices_json = app::sync_prices(
                    self.storage.clone(),
                    &self.config,
                    app::SyncPricesScopeArg::All,
                    false,
                    Some(self.config.refresh.price_staleness),
                )
                .await?;
                parse_price_counts(&prices_json)
            } else {
                (0, 0, 0)
            };

            let (symlink_connections, symlink_accounts) = if self.sync_symlinks {
                let symlink_json = app::sync_symlinks(&self.symlink_storage, &self.config).await?;
                parse_symlink_counts(&symlink_json)
            } else {
                (0, 0)
            };

            let summary = format!(
                "sync total={} synced={} manual={} fresh={} failed={} | prices fetched={} skipped={} failed={} | symlinks conn={} acct={}",
                sync_counts.total,
                sync_counts.synced,
                sync_counts.skipped_manual,
                sync_counts.skipped_not_stale,
                sync_counts.failed,
                prices_fetched,
                prices_skipped,
                prices_failed,
                symlink_connections,
                symlink_accounts,
            );

            Ok::<String, anyhow::Error>(summary)
        }
        .await;

        state.last_cycle = Some(Local::now());

        match cycle_result {
            Ok(summary) => {
                info!(summary = %summary, "keepbook daemon sync cycle complete");
                state.status = DaemonStatus::Idle;
                state.last_summary = summary;
            }
            Err(err) => {
                warn!(error = %err, "keepbook daemon sync cycle failed");
                state.status = DaemonStatus::Error(err.to_string());
                state.last_summary = format!("Cycle failed: {err}");
            }
        }

        self.refresh_history_lines(state).await;
        self.refresh_spending_lines(state).await;
        apply_tray_state(tray_handle, state).await;
    }

    async fn run(self) -> Result<()> {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();

        let mut tray_state = KeepbookTrayState::default();
        self.refresh_history_lines(&mut tray_state).await;
        self.refresh_spending_lines(&mut tray_state).await;

        let mut tray_handle = match KeepbookTray::new(tray_state.clone(), cmd_tx)
            .assume_sni_available(true)
            .spawn()
            .await
        {
            Ok(handle) => Some(handle),
            Err(err) => {
                warn!(error = %err, "Unable to start tray; daemon will continue headless");
                None
            }
        };

        apply_tray_state(&mut tray_handle, &tray_state).await;

        if self.sync_on_start {
            self.run_cycle("startup", &mut tray_state, &mut tray_handle)
                .await;
        }

        let mut next_delay = compute_next_delay(self.interval, self.jitter);
        tray_state.next_cycle = Some(local_now_plus(next_delay));
        apply_tray_state(&mut tray_handle, &tray_state).await;

        loop {
            let sleep = tokio::time::sleep(next_delay);
            tokio::pin!(sleep);

            tokio::select! {
                _ = &mut sleep => {
                    self.run_cycle("scheduled", &mut tray_state, &mut tray_handle).await;
                    next_delay = compute_next_delay(self.interval, self.jitter);
                    tray_state.next_cycle = Some(local_now_plus(next_delay));
                    apply_tray_state(&mut tray_handle, &tray_state).await;
                }
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        DaemonCommand::SyncNow => {
                            self.run_cycle("manual", &mut tray_state, &mut tray_handle).await;
                            next_delay = compute_next_delay(self.interval, self.jitter);
                            tray_state.next_cycle = Some(local_now_plus(next_delay));
                            apply_tray_state(&mut tray_handle, &tray_state).await;
                        }
                        DaemonCommand::Quit => {
                            if let Some(handle) = tray_handle.as_ref() {
                                handle.shutdown().await;
                            }
                            break;
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    if let Some(handle) = tray_handle.as_ref() {
                        handle.shutdown().await;
                    }
                    break;
                }
            }
        }

        Ok(())
    }
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

    let daemon = Daemon {
        storage,
        symlink_storage: storage_impl,
        config,
        interval: cli.interval,
        jitter: cli.jitter,
        sync_on_start: !cli.no_sync_on_start,
        sync_prices: !cli.no_sync_prices,
        sync_symlinks: !cli.no_sync_symlinks,
        history_points: cli.history_points,
    };

    daemon.run().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_next_delay_without_jitter_is_constant() {
        let interval = Duration::from_secs(1800);
        let jitter = Duration::ZERO;
        let delay = compute_next_delay(interval, jitter);
        assert_eq!(delay, interval);
    }

    #[test]
    fn compute_next_delay_with_jitter_stays_in_range() {
        let interval = Duration::from_secs(600);
        let jitter = Duration::from_secs(120);

        for _ in 0..100 {
            let delay = compute_next_delay(interval, jitter);
            assert!(delay >= Duration::from_secs(480));
            assert!(delay <= Duration::from_secs(720));
        }
    }

    #[test]
    fn parse_sync_counts_handles_mixed_results() {
        let value = serde_json::json!({
            "total": 4,
            "results": [
                {"success": true},
                {"success": true, "skipped": true, "reason": "manual"},
                {"success": true, "skipped": true, "reason": "not stale"},
                {"success": false, "error": "boom"}
            ]
        });

        let counts = parse_sync_counts(&value);
        assert_eq!(counts.total, 4);
        assert_eq!(counts.synced, 1);
        assert_eq!(counts.skipped_manual, 1);
        assert_eq!(counts.skipped_not_stale, 1);
        assert_eq!(counts.failed, 1);
    }

    #[test]
    fn format_tray_currency_uses_usd_symbol_by_default() {
        let display = keepbook::config::DisplayConfig::default();
        let formatted = format_tray_currency("1234.5", "USD", &display);
        assert_eq!(formatted, "$1234.5");
    }

    #[test]
    fn format_tray_currency_appends_unknown_currency_code() {
        let display = keepbook::config::DisplayConfig::default();
        let formatted = format_tray_currency("1234.5", "CHF", &display);
        assert_eq!(formatted, "1234.5 CHF");
    }
}

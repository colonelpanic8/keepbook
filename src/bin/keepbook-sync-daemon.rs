use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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
use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rand::Rng;
use rust_decimal::Decimal;
use std::str::FromStr;
use tokio::sync::mpsc::{self, UnboundedSender};
use tracing::{info, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

const CLI_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (git commit ",
    env!("GIT_COMMIT_HASH"),
    ")"
);

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
const DATA_WATCH_DEBOUNCE: Duration = Duration::from_millis(500);
const PORTFOLIO_GRAPH_DAYS: u32 = 90;
const PORTFOLIO_GRAPH_SPARKLINE_WIDTH: usize = 36;
const PORTFOLIO_GRAPH_MAX_RENDER_POINTS: usize = 180;

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

fn parse_nonzero_duration_arg(s: &str) -> Result<Duration, String> {
    let duration = parse_duration_arg(s)?;
    if duration.is_zero() {
        return Err("duration must be greater than 0s".to_string());
    }
    Ok(duration)
}

fn normalize_spending_windows_days(windows: &[u32]) -> Vec<u32> {
    let mut normalized: Vec<u32> = windows.iter().copied().filter(|days| *days > 0).collect();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

fn format_spending_window_label(days: u32) -> String {
    match days {
        365 => "year".to_string(),
        _ if days % 365 == 0 => format!("{} years", days / 365),
        _ => format!("{days}d"),
    }
}

fn should_refresh_for_fs_event_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

fn start_data_dir_watcher(
    data_dir: &Path,
    refresh_signal_tx: mpsc::UnboundedSender<()>,
) -> notify::Result<RecommendedWatcher> {
    let mut watcher =
        notify::recommended_watcher(move |result: notify::Result<notify::Event>| match result {
            Ok(event) => {
                if should_refresh_for_fs_event_kind(&event.kind) {
                    let _ = refresh_signal_tx.send(());
                }
            }
            Err(err) => {
                warn!(error = %err, "data directory watch event failed");
            }
        })?;
    watcher.configure(NotifyConfig::default())?;
    watcher.watch(data_dir, RecursiveMode::Recursive)?;
    Ok(watcher)
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum DaemonStatus {
    Idle,
    Syncing,
    Error(String),
}

#[derive(Debug, Clone)]
enum DaemonCommand {
    SyncNow,
    OpenPortfolioGraph,
    Quit,
}

#[derive(Debug, Clone)]
struct KeepbookTrayState {
    status: DaemonStatus,
    last_cycle: Option<DateTime<Local>>,
    next_cycle: Option<DateTime<Local>>,
    last_summary: String,
    history_lines: Vec<String>,
    portfolio_breakdown_lines: Vec<String>,
    graph_lines: Vec<String>,
    spending_lines: Vec<String>,
    transaction_lines: Vec<String>,
}

impl Default for KeepbookTrayState {
    fn default() -> Self {
        Self {
            status: DaemonStatus::Idle,
            last_cycle: None,
            next_cycle: None,
            last_summary: "No sync cycle has run yet".to_string(),
            history_lines: vec!["No portfolio history loaded".to_string()],
            portfolio_breakdown_lines: vec!["No portfolio breakdown loaded".to_string()],
            graph_lines: vec!["No portfolio graph loaded".to_string()],
            spending_lines: vec!["Spending metrics not loaded".to_string()],
            transaction_lines: vec!["Transactions not loaded".to_string()],
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

        let spending_items: Vec<MenuItem<Self>> = if self.state.spending_lines.is_empty() {
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

        let portfolio_breakdown_menu: Vec<MenuItem<Self>> =
            if self.state.portfolio_breakdown_lines.is_empty() {
                vec![StandardItem {
                    label: "No portfolio breakdown available".to_string(),
                    enabled: false,
                    ..Default::default()
                }
                .into()]
            } else {
                self.state
                    .portfolio_breakdown_lines
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

        let graph_items: Vec<MenuItem<Self>> = if self.state.graph_lines.is_empty() {
            vec![StandardItem {
                label: "No portfolio graph available".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into()]
        } else {
            self.state
                .graph_lines
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
        ];

        items.push(
            SubMenu {
                label: "Portfolio Breakdown".to_string(),
                icon_name: "view-financial-account".to_string(),
                submenu: portfolio_breakdown_menu,
                ..Default::default()
            }
            .into(),
        );
        items.push(
            StandardItem {
                label: format!("Portfolio Graph (last {PORTFOLIO_GRAPH_DAYS}d)"),
                enabled: false,
                ..Default::default()
            }
            .into(),
        );
        items.extend(graph_items);
        items.push(
            StandardItem {
                label: "Open Detailed Portfolio Graph".to_string(),
                icon_name: "office-chart-line".to_string(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.cmd_tx.send(DaemonCommand::OpenPortfolioGraph);
                }),
                ..Default::default()
            }
            .into(),
        );
        items.push(
            StandardItem {
                label: "Recent Spending".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into(),
        );

        // Keep spending metrics as top-level rows (not nested in a submenu).
        items.extend(spending_items);

        let transaction_menu: Vec<MenuItem<Self>> = if self.state.transaction_lines.is_empty() {
            vec![StandardItem {
                label: "No recent transactions".to_string(),
                enabled: false,
                ..Default::default()
            }
            .into()]
        } else {
            self.state
                .transaction_lines
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

        items.push(
            SubMenu {
                label: "Recent Transactions".to_string(),
                submenu: transaction_menu,
                ..Default::default()
            }
            .into(),
        );

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

fn format_history_change_for_tray(percentage_change: Option<&str>) -> String {
    match percentage_change {
        Some("N/A") | None => "N/A".to_string(),
        Some(value) if value.starts_with('-') => format!("{value}%"),
        Some(value) => format!("+{value}%"),
    }
}

#[derive(Clone, Debug)]
struct GraphPoint {
    date: String,
    value_str: String,
    value: f64,
}

fn parse_graph_value(value: &str) -> Option<f64> {
    value.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn downsample_graph_points(points: &[app::HistoryPoint], max_points: usize) -> Vec<GraphPoint> {
    let parsed: Vec<GraphPoint> = points
        .iter()
        .filter_map(|point| {
            parse_graph_value(&point.total_value).map(|value| GraphPoint {
                date: point.date.clone(),
                value_str: point.total_value.clone(),
                value,
            })
        })
        .collect();

    if parsed.is_empty() || max_points == 0 {
        return Vec::new();
    }

    if parsed.len() <= max_points {
        return parsed;
    }

    if max_points == 1 {
        return vec![parsed[parsed.len() - 1].clone()];
    }

    let step = (parsed.len() - 1) as f64 / (max_points - 1) as f64;
    (0..max_points)
        .map(|index| {
            let source_index = ((index as f64) * step).round() as usize;
            parsed[source_index.min(parsed.len() - 1)].clone()
        })
        .collect()
}

fn build_sparkline(values: &[f64]) -> String {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    if values.is_empty() {
        return "No graph data".to_string();
    }

    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    if !min.is_finite() || !max.is_finite() {
        return "No graph data".to_string();
    }

    if (max - min).abs() < f64::EPSILON {
        return std::iter::repeat(BLOCKS[3]).take(values.len()).collect();
    }

    values
        .iter()
        .map(|value| {
            let ratio = ((*value - min) / (max - min)).clamp(0.0, 1.0);
            let bucket = (ratio * (BLOCKS.len() - 1) as f64).round() as usize;
            BLOCKS[bucket.min(BLOCKS.len() - 1)]
        })
        .collect()
}

fn format_axis_currency(
    value: f64,
    currency: &str,
    display: &keepbook::config::DisplayConfig,
) -> String {
    format_tray_currency(&format!("{value:.2}"), currency, display)
}

fn build_portfolio_graph_lines(
    history: &app::HistoryOutput,
    config: &ResolvedConfig,
) -> Vec<String> {
    let points = downsample_graph_points(&history.points, PORTFOLIO_GRAPH_SPARKLINE_WIDTH);
    if points.is_empty() {
        return vec![format!(
            "No portfolio data for last {PORTFOLIO_GRAPH_DAYS}d"
        )];
    }

    let values: Vec<f64> = points.iter().map(|point| point.value).collect();
    let low = points
        .iter()
        .min_by(|left, right| left.value.total_cmp(&right.value))
        .expect("points is not empty");
    let now = points.last().expect("points is not empty");
    let delta = format_history_change_for_tray(
        history
            .summary
            .as_ref()
            .map(|summary| summary.percentage_change.as_str()),
    );

    vec![
        format!(
            "{} .. {}",
            points.first().expect("points is not empty").date,
            now.date
        ),
        build_sparkline(&values),
        format!(
            "Low {} | Now {} | {}",
            format_tray_currency(&low.value_str, &history.currency, &config.display),
            format_tray_currency(&now.value_str, &history.currency, &config.display),
            delta
        ),
    ]
}

fn build_portfolio_breakdown_lines(
    snapshot: &keepbook::portfolio::PortfolioSnapshot,
    config: &ResolvedConfig,
) -> Vec<String> {
    let mut lines = vec![format!(
        "Total: {}",
        format_tray_currency(&snapshot.total_value, &snapshot.currency, &config.display)
    )];

    let Some(accounts) = snapshot.by_account.as_ref() else {
        lines.push("No account breakdown available".to_string());
        return lines;
    };

    if accounts.is_empty() {
        lines.push("No accounts with balances".to_string());
        return lines;
    }

    lines.extend(accounts.iter().map(|account| {
        let value = account
            .value_in_base
            .as_deref()
            .map(|value| format_tray_currency(value, &snapshot.currency, &config.display))
            .unwrap_or_else(|| "unpriced".to_string());
        format!(
            "{} / {}: {}",
            account.connection_name, account.account_name, value
        )
    }));

    lines
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_portfolio_graph_html(
    history: &app::HistoryOutput,
    config: &ResolvedConfig,
) -> Result<String> {
    let points = downsample_graph_points(&history.points, PORTFOLIO_GRAPH_MAX_RENDER_POINTS);
    if points.len() < 2 {
        anyhow::bail!("Need at least two history points to render a graph");
    }

    let width = 1400.0;
    let height = 900.0;
    let left = 130.0;
    let right = 48.0;
    let top = 72.0;
    let bottom = 118.0;
    let plot_width = width - left - right;
    let plot_height = height - top - bottom;
    let plot_bottom = top + plot_height;

    let min_value = points
        .iter()
        .map(|point| point.value)
        .fold(f64::INFINITY, f64::min);
    let max_value = points
        .iter()
        .map(|point| point.value)
        .fold(f64::NEG_INFINITY, f64::max);
    let raw_range = (max_value - min_value).abs();
    let padding = if raw_range < 1.0 {
        max_value.abs().max(1.0) * 0.05
    } else {
        raw_range * 0.08
    };
    let display_min = min_value - padding;
    let display_max = max_value + padding;
    let display_range = (display_max - display_min).max(1.0);

    let coords: Vec<(f64, f64)> = points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let x_ratio = if points.len() == 1 {
                0.0
            } else {
                index as f64 / (points.len() - 1) as f64
            };
            let x = left + x_ratio * plot_width;
            let y = top + ((display_max - point.value) / display_range) * plot_height;
            (x, y)
        })
        .collect();

    let polyline_points = coords
        .iter()
        .map(|(x, y)| format!("{x:.2},{y:.2}"))
        .collect::<Vec<_>>()
        .join(" ");
    let first_x = coords.first().expect("coords is not empty").0;
    let last_x = coords.last().expect("coords is not empty").0;
    let area_points =
        format!("{first_x:.2},{plot_bottom:.2} {polyline_points} {last_x:.2},{plot_bottom:.2}");

    let grid_lines = (0..=4)
        .map(|index| {
            let ratio = index as f64 / 4.0;
            let y = top + ratio * plot_height;
            let value = display_max - ratio * (display_max - display_min);
            format!(
                r#"<g>
  <line x1="{left:.2}" y1="{y:.2}" x2="{x2:.2}" y2="{y:.2}" />
  <text x="{label_x:.2}" y="{text_y:.2}">{label}</text>
</g>"#,
                x2 = width - right,
                label_x = left - 14.0,
                text_y = y + 5.0,
                label = escape_html(&format_axis_currency(
                    value,
                    &history.currency,
                    &config.display
                ))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mid_index = points.len() / 2;
    let x_labels = [
        (coords[0].0, points[0].date.as_str()),
        (coords[mid_index].0, points[mid_index].date.as_str()),
        (
            coords[points.len() - 1].0,
            points[points.len() - 1].date.as_str(),
        ),
    ]
    .into_iter()
    .map(|(x, label)| {
        format!(
            r#"<text x="{x:.2}" y="{y:.2}" text-anchor="middle">{}</text>"#,
            escape_html(label),
            y = height - 44.0
        )
    })
    .collect::<Vec<_>>()
    .join("\n");

    let latest = points.last().expect("points is not empty");
    let low = points
        .iter()
        .min_by(|left, right| left.value.total_cmp(&right.value))
        .expect("points is not empty");
    let high = points
        .iter()
        .max_by(|left, right| left.value.total_cmp(&right.value))
        .expect("points is not empty");
    let delta = format_history_change_for_tray(
        history
            .summary
            .as_ref()
            .map(|summary| summary.percentage_change.as_str()),
    );
    let title = format!("Keepbook Net Worth (last {PORTFOLIO_GRAPH_DAYS}d)");
    let subtitle = format!(
        "{} .. {}",
        points.first().expect("points is not empty").date,
        latest.date
    );
    let final_marker = coords.last().expect("coords is not empty");

    Ok(format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{title}</title>
    <style>
      :root {{
        color-scheme: light;
        --bg: #f5f8fb;
        --panel: #ffffff;
        --ink: #10243a;
        --muted: #5f7388;
        --grid: #d8e3ef;
        --accent: #1f9d70;
        --accent-fill: rgba(31, 157, 112, 0.16);
      }}
      * {{ box-sizing: border-box; }}
      body {{
        margin: 0;
        font-family: "IBM Plex Sans", "Avenir Next", sans-serif;
        color: var(--ink);
        background:
          radial-gradient(circle at top left, rgba(31, 157, 112, 0.14), transparent 36%),
          linear-gradient(180deg, #fbfdff 0%, var(--bg) 100%);
      }}
      main {{
        max-width: 1500px;
        margin: 0 auto;
        padding: 36px 28px 48px;
      }}
      .panel {{
        background: var(--panel);
        border: 1px solid #dce7f1;
        border-radius: 24px;
        box-shadow: 0 24px 80px rgba(16, 36, 58, 0.08);
        overflow: hidden;
      }}
      header {{
        padding: 28px 32px 8px;
      }}
      h1 {{
        margin: 0;
        font-size: 30px;
        line-height: 1.1;
      }}
      .sub {{
        margin-top: 8px;
        color: var(--muted);
        font-size: 16px;
      }}
      .stats {{
        display: flex;
        gap: 18px;
        flex-wrap: wrap;
        padding: 0 32px 24px;
      }}
      .stat {{
        min-width: 180px;
        padding: 14px 16px;
        border-radius: 16px;
        background: #f7fafc;
        border: 1px solid #e4edf5;
      }}
      .stat-label {{
        color: var(--muted);
        font-size: 12px;
        letter-spacing: 0.08em;
        text-transform: uppercase;
      }}
      .stat-value {{
        margin-top: 6px;
        font-size: 21px;
        font-weight: 700;
      }}
      svg {{
        display: block;
        width: 100%;
        height: auto;
      }}
      .grid line {{
        stroke: var(--grid);
        stroke-width: 1;
      }}
      .grid text, .axis text {{
        fill: var(--muted);
        font-size: 16px;
      }}
      .area {{
        fill: var(--accent-fill);
      }}
      .line {{
        fill: none;
        stroke: var(--accent);
        stroke-width: 4;
        stroke-linejoin: round;
        stroke-linecap: round;
      }}
      .marker {{
        fill: var(--accent);
      }}
    </style>
  </head>
  <body>
    <main>
      <section class="panel">
        <header>
          <h1>{title}</h1>
          <div class="sub">{subtitle}</div>
        </header>
        <div class="stats">
          <div class="stat">
            <div class="stat-label">Current</div>
            <div class="stat-value">{current}</div>
          </div>
          <div class="stat">
            <div class="stat-label">Low</div>
            <div class="stat-value">{low}</div>
          </div>
          <div class="stat">
            <div class="stat-label">High</div>
            <div class="stat-value">{high}</div>
          </div>
          <div class="stat">
            <div class="stat-label">Change</div>
            <div class="stat-value">{delta}</div>
          </div>
        </div>
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:.0} {height:.0}" role="img" aria-labelledby="title desc">
          <title id="title">{title}</title>
          <desc id="desc">Net worth history from {subtitle}.</desc>
          <g class="grid">
            {grid_lines}
          </g>
          <polygon class="area" points="{area_points}" />
          <polyline class="line" points="{polyline_points}" />
          <circle class="marker" cx="{final_x:.2}" cy="{final_y:.2}" r="7" />
          <g class="axis">
            {x_labels}
          </g>
        </svg>
      </section>
    </main>
  </body>
</html>"#,
        title = escape_html(&title),
        subtitle = escape_html(&subtitle),
        current = escape_html(&format_tray_currency(
            &latest.value_str,
            &history.currency,
            &config.display
        )),
        low = escape_html(&format_tray_currency(
            &low.value_str,
            &history.currency,
            &config.display
        )),
        high = escape_html(&format_tray_currency(
            &high.value_str,
            &history.currency,
            &config.display
        )),
        delta = escape_html(&delta),
        grid_lines = grid_lines,
        area_points = area_points,
        polyline_points = polyline_points,
        final_x = final_marker.0,
        final_y = final_marker.1,
        x_labels = x_labels
    ))
}

fn portfolio_graph_output_path() -> Result<PathBuf> {
    let base = dirs::cache_dir()
        .context("Could not find a cache directory for tray graph output")?
        .join("keepbook")
        .join("tray");
    std::fs::create_dir_all(&base)
        .with_context(|| format!("Failed to create tray graph directory: {}", base.display()))?;
    Ok(base.join(format!("portfolio-graph-last-{PORTFOLIO_GRAPH_DAYS}d.html")))
}

fn open_path_in_browser(path: &Path) -> Result<()> {
    let candidates: [(&str, Vec<OsString>); 3] = [
        ("xdg-open", vec![path.as_os_str().to_os_string()]),
        (
            "gio",
            vec![OsString::from("open"), path.as_os_str().to_os_string()],
        ),
        ("open", vec![path.as_os_str().to_os_string()]),
    ];

    let mut errors = Vec::new();

    for (program, args) in candidates {
        match Command::new(program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(_) => return Ok(()),
            Err(err) => errors.push(format!("{program}: {err}")),
        }
    }

    anyhow::bail!(
        "Unable to open portfolio graph automatically ({})",
        errors.join("; ")
    )
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
    refresh_interval: Duration,
    sync_on_start: bool,
    sync_prices: bool,
    sync_symlinks: bool,
    history_points: usize,
    spending_windows_days: Vec<u32>,
    transaction_count: usize,
}

impl Daemon {
    fn last_n_days_range(days: u32) -> (NaiveDate, NaiveDate) {
        let end = Local::now().date_naive();
        let start = end - chrono::Duration::days(days.saturating_sub(1) as i64);
        (start, end)
    }

    async fn spending_line_for_days(&self, days: u32) -> String {
        let label = format_spending_window_label(days);
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
                    "Last {label}: {} ({} {})",
                    value, report.transaction_count, tx_label
                )
            }
            Err(err) => {
                warn!(
                    window_days = days,
                    error = %err,
                    "unable to refresh tray spending metrics"
                );
                format!("Last {label}: unavailable")
            }
        }
    }

    async fn refresh_spending_lines(&self, state: &mut KeepbookTrayState) {
        let windows = normalize_spending_windows_days(&self.spending_windows_days);
        let mut lines = Vec::with_capacity(windows.len().max(1));
        for days in windows {
            lines.push(self.spending_line_for_days(days).await);
        }
        if lines.is_empty() {
            lines.push("No spending windows configured".to_string());
        }
        state.spending_lines = lines;
    }

    async fn refresh_transaction_lines(&self, state: &mut KeepbookTrayState) {
        if self.transaction_count == 0 {
            state.transaction_lines = vec!["Transaction display disabled".to_string()];
            return;
        }

        let result: Result<Vec<String>> = async {
            let connections = self.storage.list_connections().await?;
            let accounts = self.storage.list_accounts().await?;

            // Build account_id -> connection name map.
            let conn_name_by_id: std::collections::HashMap<String, String> = connections
                .iter()
                .map(|c| (c.id().to_string(), c.config.name.clone()))
                .collect();
            let account_conn_name: std::collections::HashMap<String, String> = accounts
                .iter()
                .map(|a| {
                    let conn_name = conn_name_by_id
                        .get(&a.connection_id.to_string())
                        .cloned()
                        .unwrap_or_else(|| "Unknown".to_string());
                    (a.id.to_string(), conn_name)
                })
                .collect();

            let cutoff = chrono::Utc::now() - chrono::Duration::days(30);

            struct TxRow {
                timestamp: chrono::DateTime<chrono::Utc>,
                source: String,
                amount: String,
                description: String,
                asset: keepbook::models::Asset,
            }

            let mut rows: Vec<TxRow> = Vec::new();

            for account in &accounts {
                let txns = self.storage.get_transactions(&account.id).await?;
                let source = account_conn_name
                    .get(&account.id.to_string())
                    .cloned()
                    .unwrap_or_else(|| "Unknown".to_string());

                for tx in txns {
                    if tx.timestamp < cutoff {
                        continue;
                    }
                    rows.push(TxRow {
                        timestamp: tx.timestamp,
                        source: source.clone(),
                        amount: tx.amount,
                        description: tx.description,
                        asset: tx.asset,
                    });
                }
            }

            // Sort newest first.
            rows.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
            rows.truncate(self.transaction_count);

            let lines: Vec<String> = rows
                .iter()
                .map(|row| {
                    let date = row.timestamp.with_timezone(&chrono::Local).format("%m-%d");
                    let currency = match &row.asset {
                        keepbook::models::Asset::Currency { iso_code } => iso_code.as_str(),
                        keepbook::models::Asset::Equity { ticker, .. } => ticker.as_str(),
                        keepbook::models::Asset::Crypto { symbol, .. } => symbol.as_str(),
                    };
                    let amount = format_tray_currency(&row.amount, currency, &self.config.display);
                    // Truncate long descriptions (char-safe).
                    let desc: String = if row.description.chars().count() > 30 {
                        let truncated: String = row.description.chars().take(27).collect();
                        format!("{truncated}...")
                    } else {
                        row.description.clone()
                    };
                    format!("{} | {} | {} | {}", date, row.source, amount, desc)
                })
                .collect();

            Ok(lines)
        }
        .await;

        match result {
            Ok(lines) if lines.is_empty() => {
                state.transaction_lines = vec!["No transactions in last 30 days".to_string()];
            }
            Ok(lines) => {
                state.transaction_lines = lines;
            }
            Err(err) => {
                warn!(error = %err, "unable to refresh tray transaction lines");
                state.transaction_lines = vec![format!("Transactions unavailable: {err}")];
            }
        }
    }

    async fn refresh_history_lines(&self, state: &mut KeepbookTrayState) {
        match app::portfolio_recent_history(
            self.storage.clone(),
            &self.config,
            None,
            true,
            Local::now().date_naive(),
        )
        .await
        {
            Ok(history_points) => {
                let mut lines: Vec<String> = history_points
                    .iter()
                    .rev()
                    .take(self.history_points)
                    .map(|point| {
                        let value = format_tray_currency(
                            &point.total_value,
                            &self.config.reporting_currency,
                            &self.config.display,
                        );
                        let percentage_change = format_history_change_for_tray(
                            point.percentage_change_from_previous.as_deref(),
                        );
                        format!("{}: {} ({} vs prev)", point.date, value, percentage_change)
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

    async fn refresh_portfolio_breakdown_lines(&self, state: &mut KeepbookTrayState) {
        match app::portfolio_snapshot(
            self.storage.clone(),
            &self.config,
            None,
            None,
            "account".to_string(),
            false,
            None,
            false,
            true,
            false,
            false,
        )
        .await
        {
            Ok(snapshot) => {
                state.portfolio_breakdown_lines =
                    build_portfolio_breakdown_lines(&snapshot, &self.config);
            }
            Err(err) => {
                warn!(error = %err, "unable to refresh tray portfolio breakdown");
                state.portfolio_breakdown_lines = vec![format!("Portfolio unavailable: {err}")];
            }
        }
    }

    async fn portfolio_graph_history(&self) -> Result<app::HistoryOutput> {
        let (start, end) = Self::last_n_days_range(PORTFOLIO_GRAPH_DAYS);
        app::portfolio_history(
            self.storage.clone(),
            &self.config,
            None,
            Some(start.format("%Y-%m-%d").to_string()),
            Some(end.format("%Y-%m-%d").to_string()),
            "daily".to_string(),
            true,
        )
        .await
    }

    async fn refresh_graph_lines(&self, state: &mut KeepbookTrayState) {
        match self.portfolio_graph_history().await {
            Ok(history) => {
                state.graph_lines = build_portfolio_graph_lines(&history, &self.config);
            }
            Err(err) => {
                warn!(error = %err, "unable to refresh tray portfolio graph");
                state.graph_lines = vec![format!("Graph unavailable: {err}")];
            }
        }
    }

    async fn open_portfolio_graph(&self) -> Result<PathBuf> {
        let history = self.portfolio_graph_history().await?;
        let html = render_portfolio_graph_html(&history, &self.config)?;
        let output_path = portfolio_graph_output_path()?;
        std::fs::write(&output_path, html)
            .with_context(|| format!("Failed to write graph HTML: {}", output_path.display()))?;
        open_path_in_browser(&output_path)?;
        Ok(output_path)
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

        self.refresh_tray_state(state, tray_handle).await;
    }

    async fn refresh_tray_state(
        &self,
        state: &mut KeepbookTrayState,
        tray_handle: &mut Option<ksni::Handle<KeepbookTray>>,
    ) {
        self.refresh_history_lines(state).await;
        self.refresh_portfolio_breakdown_lines(state).await;
        self.refresh_graph_lines(state).await;
        self.refresh_spending_lines(state).await;
        self.refresh_transaction_lines(state).await;
        apply_tray_state(tray_handle, state).await;
    }

    async fn run(self) -> Result<()> {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();

        let mut tray_state = KeepbookTrayState::default();
        self.refresh_history_lines(&mut tray_state).await;
        self.refresh_portfolio_breakdown_lines(&mut tray_state)
            .await;
        self.refresh_graph_lines(&mut tray_state).await;
        self.refresh_spending_lines(&mut tray_state).await;
        self.refresh_transaction_lines(&mut tray_state).await;

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

        let mut refresh_tick = tokio::time::interval(self.refresh_interval);
        refresh_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        refresh_tick.tick().await;

        let (watch_refresh_tx, mut watch_refresh_rx) = mpsc::unbounded_channel::<()>();
        let watcher = match start_data_dir_watcher(&self.config.data_dir, watch_refresh_tx) {
            Ok(watcher) => {
                info!(
                    path = %self.config.data_dir.display(),
                    debounce_ms = DATA_WATCH_DEBOUNCE.as_millis(),
                    "watching keepbook data directory for tray refresh"
                );
                Some(watcher)
            }
            Err(err) => {
                warn!(
                    error = %err,
                    path = %self.config.data_dir.display(),
                    "unable to watch data directory; relying on periodic tray refresh fallback"
                );
                None
            }
        };
        let data_watch_debounce = tokio::time::sleep(Duration::from_secs(24 * 60 * 60));
        tokio::pin!(data_watch_debounce);
        let mut data_watch_debounce_armed = false;

        let sync_sleep = tokio::time::sleep(next_delay);
        tokio::pin!(sync_sleep);

        loop {
            tokio::select! {
                _ = &mut sync_sleep => {
                    self.run_cycle("scheduled", &mut tray_state, &mut tray_handle).await;
                    next_delay = compute_next_delay(self.interval, self.jitter);
                    tray_state.next_cycle = Some(local_now_plus(next_delay));
                    apply_tray_state(&mut tray_handle, &tray_state).await;
                    sync_sleep.as_mut().reset(tokio::time::Instant::now() + next_delay);
                }
                _ = refresh_tick.tick() => {
                    self.refresh_tray_state(&mut tray_state, &mut tray_handle).await;
                }
                Some(()) = watch_refresh_rx.recv(), if watcher.is_some() => {
                    while watch_refresh_rx.try_recv().is_ok() {}
                    data_watch_debounce
                        .as_mut()
                        .reset(tokio::time::Instant::now() + DATA_WATCH_DEBOUNCE);
                    data_watch_debounce_armed = true;
                }
                _ = &mut data_watch_debounce, if data_watch_debounce_armed => {
                    data_watch_debounce_armed = false;
                    self.refresh_tray_state(&mut tray_state, &mut tray_handle).await;
                }
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        DaemonCommand::SyncNow => {
                            self.run_cycle("manual", &mut tray_state, &mut tray_handle).await;
                            next_delay = compute_next_delay(self.interval, self.jitter);
                            tray_state.next_cycle = Some(local_now_plus(next_delay));
                            apply_tray_state(&mut tray_handle, &tray_state).await;
                            sync_sleep.as_mut().reset(tokio::time::Instant::now() + next_delay);
                        }
                        DaemonCommand::OpenPortfolioGraph => {
                            match self.open_portfolio_graph().await {
                                Ok(path) => {
                                    tray_state.last_summary = format!(
                                        "Opened portfolio graph: {}",
                                        path.display()
                                    );
                                }
                                Err(err) => {
                                    warn!(error = %err, "unable to open portfolio graph");
                                    tray_state.last_summary =
                                        format!("Portfolio graph unavailable: {err}");
                                }
                            }
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
mod tests {
    use super::*;
    use ksni::Tray;
    use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind};

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
    fn parse_nonzero_duration_rejects_zero() {
        assert!(parse_nonzero_duration_arg("0s").is_err());
        assert_eq!(
            parse_nonzero_duration_arg("30s").expect("duration should parse"),
            Duration::from_secs(30)
        );
    }

    #[test]
    fn fs_event_filter_includes_state_mutations() {
        assert!(should_refresh_for_fs_event_kind(&EventKind::Any));
        assert!(should_refresh_for_fs_event_kind(&EventKind::Create(
            CreateKind::Any
        )));
        assert!(should_refresh_for_fs_event_kind(&EventKind::Modify(
            ModifyKind::Any
        )));
        assert!(should_refresh_for_fs_event_kind(&EventKind::Remove(
            RemoveKind::Any
        )));
    }

    #[test]
    fn fs_event_filter_excludes_access_events() {
        assert!(!should_refresh_for_fs_event_kind(&EventKind::Access(
            AccessKind::Any
        )));
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

    #[test]
    fn format_history_change_for_tray_defaults_to_na() {
        assert_eq!(format_history_change_for_tray(None), "N/A");
        assert_eq!(format_history_change_for_tray(Some("N/A")), "N/A");
    }

    #[test]
    fn format_history_change_for_tray_adds_sign_and_percent() {
        assert_eq!(format_history_change_for_tray(Some("3.25")), "+3.25%");
        assert_eq!(format_history_change_for_tray(Some("-1.50")), "-1.50%");
    }

    #[test]
    fn build_sparkline_tracks_simple_growth() {
        let sparkline = build_sparkline(&[10.0, 20.0, 30.0, 40.0]);
        assert_eq!(sparkline.chars().count(), 4);
        assert_eq!(sparkline.chars().next(), Some('▁'));
        assert_eq!(sparkline.chars().last(), Some('█'));
    }

    #[test]
    fn recent_spending_is_not_rendered_as_submenu() {
        let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
        let state = KeepbookTrayState {
            spending_lines: vec!["Last 7d: $42 (3 txns)".to_string()],
            ..KeepbookTrayState::default()
        };
        let tray = KeepbookTray::new(state, cmd_tx);

        let menu = tray.menu();
        assert!(
            menu.iter().any(|item| {
                matches!(
                    item,
                    MenuItem::Standard(StandardItem { label, .. }) if label == "Recent Spending"
                )
            }),
            "expected top-level standard item with label 'Recent Spending'"
        );
        assert!(
            !menu.iter().any(|item| {
                matches!(
                    item,
                    MenuItem::SubMenu(SubMenu { label, .. }) if label == "Recent Spending"
                )
            }),
            "did not expect 'Recent Spending' to be rendered as a submenu"
        );
    }

    #[test]
    fn normalize_spending_windows_days_sorts_dedupes_and_drops_zero() {
        assert_eq!(
            normalize_spending_windows_days(&[30, 0, 365, 7, 30]),
            vec![7, 30, 365]
        );
    }

    #[test]
    fn format_spending_window_label_uses_year_for_365_days() {
        assert_eq!(format_spending_window_label(7), "7d");
        assert_eq!(format_spending_window_label(365), "year");
        assert_eq!(format_spending_window_label(730), "2 years");
    }

    #[test]
    fn portfolio_graph_preview_and_open_action_are_rendered_top_level() {
        let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
        let state = KeepbookTrayState {
            graph_lines: vec![
                "2026-01-22 .. 2026-04-20".to_string(),
                "▁▂▃▄▅▆▇█".to_string(),
                "Low $100 | Now $150 | +50.00%".to_string(),
            ],
            ..KeepbookTrayState::default()
        };
        let tray = KeepbookTray::new(state, cmd_tx);

        let menu = tray.menu();
        assert!(menu.iter().any(|item| {
            matches!(
                item,
                MenuItem::Standard(StandardItem { label, .. })
                    if label == &format!("Portfolio Graph (last {PORTFOLIO_GRAPH_DAYS}d)")
            )
        }));
        assert!(menu.iter().any(|item| {
            matches!(
                item,
                MenuItem::Standard(StandardItem { label, .. })
                    if label == "Open Detailed Portfolio Graph"
            )
        }));
    }

    #[test]
    fn build_portfolio_breakdown_lines_formats_account_values() {
        let mut config =
            ResolvedConfig::load_or_default(Path::new("/tmp/keepbook-test/keepbook.toml")).unwrap();
        config.display = keepbook::config::DisplayConfig {
            currency_decimals: Some(2),
            currency_grouping: true,
            currency_symbol: Some("$".to_string()),
            currency_fixed_decimals: true,
        };
        let snapshot = keepbook::portfolio::PortfolioSnapshot {
            as_of_date: NaiveDate::from_ymd_opt(2026, 4, 24).unwrap(),
            currency: "USD".to_string(),
            total_value: "1250".to_string(),
            by_asset: None,
            by_account: Some(vec![
                keepbook::portfolio::AccountSummary {
                    account_id: "acct-1".to_string(),
                    account_name: "Checking".to_string(),
                    connection_name: "Bank".to_string(),
                    value_in_base: Some("1000".to_string()),
                },
                keepbook::portfolio::AccountSummary {
                    account_id: "acct-2".to_string(),
                    account_name: "Brokerage".to_string(),
                    connection_name: "Broker".to_string(),
                    value_in_base: None,
                },
            ]),
        };

        let lines = build_portfolio_breakdown_lines(&snapshot, &config);

        assert_eq!(lines[0], "Total: $1,250.00");
        assert_eq!(lines[1], "Bank / Checking: $1,000.00");
        assert_eq!(lines[2], "Broker / Brokerage: unpriced");
    }

    #[test]
    fn portfolio_breakdown_is_rendered_as_submenu() {
        let (cmd_tx, _cmd_rx) = mpsc::unbounded_channel();
        let state = KeepbookTrayState {
            portfolio_breakdown_lines: vec![
                "Total: $42.00".to_string(),
                "Bank / Checking: $42.00".to_string(),
            ],
            ..KeepbookTrayState::default()
        };
        let tray = KeepbookTray::new(state, cmd_tx);

        let menu = tray.menu();
        assert!(menu.iter().any(|item| {
            matches!(
                item,
                MenuItem::SubMenu(SubMenu { label, submenu, .. })
                    if label == "Portfolio Breakdown" && submenu.len() == 2
            )
        }));
    }
}

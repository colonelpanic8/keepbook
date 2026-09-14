use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Local, NaiveDate};
use keepbook::app;
use keepbook::app::tray::{
    build_portfolio_breakdown_lines, format_history_change_for_tray, format_spending_window_label,
    format_tray_currency, normalize_spending_windows_days,
};
use keepbook::config::ResolvedConfig;
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::TransactionSyncMode;
use ksni::TrayMethods;
use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rand::Rng;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::activation::open_dioxus_app;
use crate::parse::{parse_price_counts, parse_symlink_counts, parse_sync_counts};
use crate::tray::{apply_tray_state, DaemonCommand, DaemonStatus, KeepbookTray, KeepbookTrayState};

const DATA_WATCH_DEBOUNCE: Duration = Duration::from_millis(500);

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

pub(crate) struct Daemon {
    pub(crate) storage: Arc<dyn Storage>,
    pub(crate) symlink_storage: JsonFileStorage,
    pub(crate) config: ResolvedConfig,
    pub(crate) interval: Duration,
    pub(crate) jitter: Duration,
    pub(crate) refresh_interval: Duration,
    pub(crate) sync_on_start: bool,
    pub(crate) sync_prices: bool,
    pub(crate) sync_symlinks: bool,
    pub(crate) history_points: usize,
    pub(crate) spending_windows_days: Vec<u32>,
    pub(crate) transaction_count: usize,
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
            period_alignment: None,
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
            rows.sort_by_key(|row| std::cmp::Reverse(row.timestamp));
            rows.truncate(self.transaction_count);

            let lines: Vec<String> = rows
                .iter()
                .map(|row| {
                    let date = row.timestamp.with_timezone(&chrono::Local).format("%m-%d");
                    let currency = match &row.asset {
                        keepbook::models::Asset::Currency { iso_code } => iso_code.as_str(),
                        keepbook::models::Asset::ManualValue { currency, .. } => currency.as_str(),
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
            app::PortfolioSnapshotRequest {
                group_by: "account".to_string(),
                offline: true,
                ..Default::default()
            },
        )
        .await
        {
            Ok(snapshot) => {
                state.portfolio_breakdown_lines =
                    build_portfolio_breakdown_lines(&snapshot, &self.config.display);
            }
            Err(err) => {
                warn!(error = %err, "unable to refresh tray portfolio breakdown");
                state.portfolio_breakdown_lines = vec![format!("Portfolio unavailable: {err}")];
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
                    pull_remote: self.config.git.pull_before_edit,
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

            app::maybe_push_after_sync(&self.config, self.config.git.push_after_sync)?;

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
        self.refresh_spending_lines(state).await;
        self.refresh_transaction_lines(state).await;
        apply_tray_state(tray_handle, state).await;
    }

    pub(crate) async fn run(self) -> Result<()> {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();

        let mut tray_state = KeepbookTrayState::default();
        self.refresh_history_lines(&mut tray_state).await;
        self.refresh_portfolio_breakdown_lines(&mut tray_state)
            .await;
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
                        DaemonCommand::OpenDioxusApp => {
                            match open_dioxus_app() {
                                Ok(()) => {
                                    tray_state.last_summary =
                                        "Opened Dioxus app".to_string();
                                }
                                Err(err) => {
                                    warn!(error = %err, "unable to open Dioxus app");
                                    tray_state.last_summary =
                                        format!("Dioxus app unavailable: {err}");
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

#[cfg(test)]
#[path = "../../../tests/unit/bin/keepbook_sync_daemon/daemon_tests.rs"]
mod daemon_tests;

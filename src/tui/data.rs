//! Loading and refreshing the data the TUI displays.

use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;

use crate::app::transaction_tag_rules::load_transaction_tag_rules;
use crate::app::{self, HistoryPoint, TransactionOutput};
use crate::config::ResolvedConfig;
use crate::storage::Storage;

use super::state::{AppState, NetWorthInterval};

const LOAD_START_DATE: &str = "1900-01-01";
const LOAD_END_DATE: &str = "9999-12-31";

pub(super) async fn refresh_transactions_and_rules(
    app_state: &mut AppState,
    storage: &dyn Storage,
    config: &ResolvedConfig,
) -> Result<()> {
    app_state.all_transactions =
        load_transactions(storage, config, app_state.include_ignored).await?;
    app_state.transaction_last_refresh_utc = Utc::now();
    app_state.recompute_visible_transactions();

    let (matcher, warning) = load_transaction_tag_rules(&app_state.tag_rules_path)?;
    app_state.tag_matcher = matcher;
    if warning.is_some() {
        app_state.status_message = warning;
    }
    Ok(())
}

pub(super) async fn load_transactions(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    include_ignored: bool,
) -> Result<Vec<TransactionOutput>> {
    app::list_transactions(
        storage,
        Some(LOAD_START_DATE.to_string()),
        Some(LOAD_END_DATE.to_string()),
        None,
        false,
        !include_ignored,
        config,
    )
    .await
}

async fn load_net_worth(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    interval: NetWorthInterval,
) -> Result<Vec<HistoryPoint>> {
    let output = app::portfolio_history(
        storage,
        config,
        None,
        None,
        None,
        interval.as_granularity().to_string(),
        true,
        false,
    )
    .await?;
    Ok(output.points)
}

pub(super) async fn refresh_net_worth(
    app_state: &mut AppState,
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
) {
    let refreshed_at = Utc::now();
    let result = load_net_worth(storage, config, app_state.net_worth_interval).await;
    app_state.net_worth_loaded = true;
    app_state.net_worth_last_refresh_utc = refreshed_at;

    match result {
        Ok(points) => {
            app_state.net_worth_points = points;
            app_state.net_worth_error = None;
            app_state.recompute_visible_net_worth();
        }
        Err(error) => {
            app_state.net_worth_points.clear();
            app_state.visible_net_worth_indices.clear();
            app_state.net_worth_error = Some(error.to_string());
        }
    }
}

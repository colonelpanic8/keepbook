use std::sync::Arc;

use anyhow::Result;
use keepbook::config::ResolvedConfig;
use keepbook::storage::Storage;

use crate::cli::{NetWorthIntervalArg, TuiViewArg};

pub async fn run(
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
    view: TuiViewArg,
    net_worth_interval: NetWorthIntervalArg,
) -> Result<()> {
    keepbook::tui::run_tui(
        storage_arc.clone(),
        config,
        keepbook::tui::TuiOptions {
            start_view: view.into(),
            net_worth_interval: net_worth_interval.into(),
        },
    )
    .await?;
    Ok(())
}

use std::sync::Arc;

use anyhow::Result;
use keepbook::app;
use keepbook::config::ResolvedConfig;
use keepbook::storage::Storage;

use crate::cli::MarketDataCommand;

pub async fn run(
    command: MarketDataCommand,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    match command {
        MarketDataCommand::Fetch {
            account,
            connection,
            start,
            end,
            interval,
            lookback_days,
            request_delay_ms,
            currency,
            no_fx,
        } => {
            let output = app::fetch_historical_prices(app::PriceHistoryRequest {
                storage: storage_arc.as_ref(),
                config,
                account: account.as_deref(),
                connection: connection.as_deref(),
                start: start.as_deref(),
                end: end.as_deref(),
                interval: interval.as_str(),
                lookback_days,
                request_delay_ms,
                currency,
                include_fx: !no_fx,
            })
            .await?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }
    Ok(())
}

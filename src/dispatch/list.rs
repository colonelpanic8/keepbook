use std::sync::Arc;

use anyhow::Result;
use keepbook::app;
use keepbook::config::ResolvedConfig;
use keepbook::storage::Storage;

use crate::cli::ListCommand;

pub async fn run(
    command: ListCommand,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    match command {
        ListCommand::Connections => {
            let connections = app::list_connections(storage_arc.as_ref()).await?;
            println!("{}", serde_json::to_string_pretty(&connections)?);
        }

        ListCommand::Accounts => {
            let accounts = app::list_accounts(storage_arc.as_ref()).await?;
            println!("{}", serde_json::to_string_pretty(&accounts)?);
        }

        ListCommand::PriceSources => {
            let sources = app::list_price_sources(&config.data_dir)?;
            println!("{}", serde_json::to_string_pretty(&sources)?);
        }

        ListCommand::Balances => {
            let balances = app::list_balances(storage_arc.as_ref(), config).await?;
            println!("{}", serde_json::to_string_pretty(&balances)?);
        }

        ListCommand::Transactions {
            start,
            end,
            sort_by_amount,
            include_ignored,
        } => {
            let transactions = app::list_transactions(
                storage_arc.as_ref(),
                start,
                end,
                None,
                sort_by_amount,
                !include_ignored,
                config,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&transactions)?);
        }

        ListCommand::RecurringTransactions {
            start,
            end,
            include_ignored,
            include_possible,
            min_confidence,
            include_dismissed,
        } => {
            let recurring = app::list_reviewed_recurring_transactions(
                storage_arc.as_ref(),
                app::RecurringTransactionsOptions {
                    start,
                    end,
                    include_ignored,
                    include_possible,
                    min_confidence,
                },
                include_dismissed,
                config,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&recurring)?);
        }

        ListCommand::All => {
            let output = app::list_all(storage_arc.as_ref(), config).await?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }
    Ok(())
}

use std::sync::Arc;

use anyhow::Result;
use keepbook::app;
use keepbook::config::ResolvedConfig;
use keepbook::storage::Storage;

use crate::cli::PortfolioCommand;

pub async fn run(
    command: PortfolioCommand,
    storage_arc: &Arc<dyn Storage>,
    config: &ResolvedConfig,
) -> Result<()> {
    match command {
        PortfolioCommand::Snapshot {
            currency,
            date,
            group_by,
            detail,
            capital_gains_tax_rate,
            equity_change_percent,
            target_pre_tax_total_value,
            auto,
            offline,
            dry_run,
            force_refresh,
        } => {
            let snapshot = app::portfolio_snapshot(
                storage_arc.clone(),
                config,
                app::PortfolioSnapshotRequest {
                    currency,
                    date,
                    group_by,
                    detail,
                    capital_gains_tax_rate,
                    equity_change_percent,
                    target_pre_tax_total_value,
                    auto,
                    offline,
                    dry_run,
                    force_refresh,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }

        PortfolioCommand::Assets {
            date,
            include_amount_changes,
        } => {
            let output =
                app::portfolio_assets(storage_arc.clone(), config, date, include_amount_changes)
                    .await?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        PortfolioCommand::TaxImpact {
            currency,
            date,
            capital_gains_tax_rate,
            min,
            max,
            points,
        } => {
            let output = app::portfolio_tax_impact(
                storage_arc.clone(),
                config,
                app::PortfolioTaxImpactRequest {
                    currency,
                    date,
                    capital_gains_tax_rate,
                    min,
                    max,
                    points,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        PortfolioCommand::History {
            currency,
            start,
            end,
            granularity,
            include_prices,
            no_include_prices,
            account,
            connection,
        } => {
            let granularity =
                granularity.unwrap_or_else(|| config.history.portfolio_granularity.clone());
            let include_prices = if no_include_prices {
                false
            } else if include_prices {
                true
            } else {
                config.history.include_prices
            };
            let selection = app::resolve_portfolio_history_selection(
                storage_arc.as_ref(),
                config,
                account.as_deref(),
                connection.as_deref(),
            )
            .await?;
            let output = match selection {
                app::PortfolioHistorySelection::Portfolio => {
                    app::portfolio_history(
                        storage_arc.clone(),
                        config,
                        currency,
                        start,
                        end,
                        granularity,
                        include_prices,
                        false,
                    )
                    .await?
                }
                app::PortfolioHistorySelection::Accounts(account_ids) => {
                    app::portfolio_history_for_accounts(
                        storage_arc.clone(),
                        config,
                        currency,
                        start,
                        end,
                        granularity,
                        include_prices,
                        false,
                        account_ids,
                    )
                    .await?
                }
                app::PortfolioHistorySelection::LatentCapitalGainsTax => {
                    app::latent_capital_gains_tax_history(
                        storage_arc.clone(),
                        config,
                        currency,
                        start,
                        end,
                        granularity,
                        include_prices,
                        false,
                    )
                    .await?
                }
            };
            println!("{}", serde_json::to_string_pretty(&output)?);
        }

        PortfolioCommand::ChangePoints {
            start,
            end,
            granularity,
            include_prices,
            no_include_prices,
        } => {
            let granularity =
                granularity.unwrap_or_else(|| config.history.change_points_granularity.clone());
            let include_prices = if no_include_prices {
                false
            } else if include_prices {
                true
            } else {
                config.history.include_prices
            };
            let output = app::portfolio_change_points(
                storage_arc.clone(),
                config,
                start,
                end,
                granularity,
                include_prices,
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }
    Ok(())
}

// src/portfolio/service.rs
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;

use crate::clock::{Clock, SystemClock};
use crate::format::format_base_currency_value;
use crate::market_data::{AssetId, MarketDataService};
use crate::models::{Account, Asset, BalanceBackfillPolicy, BalanceSnapshot, Connection, Id};
use crate::storage::Storage;

use super::{
    AccountHolding, AccountSummary, AssetSummary, Grouping, PortfolioQuery, PortfolioSnapshot,
};

pub struct PortfolioService {
    storage: Arc<dyn Storage>,
    market_data: Arc<MarketDataService>,
    clock: Arc<dyn Clock>,
}

/// Valuation result for an asset.
struct AssetValuation {
    /// The value in target currency. None if price data unavailable.
    value: Option<Decimal>,
    price: Option<String>,
    price_date: Option<NaiveDate>,
    price_timestamp: Option<DateTime<Utc>>,
    fx_rate: Option<String>,
    fx_date: Option<NaiveDate>,
}

/// Represents a single asset holding from a snapshot.
struct AssetHolding {
    account_id: Id,
    #[allow(dead_code)]
    asset: Asset,
    amount: String,
    timestamp: DateTime<Utc>,
}

/// Aggregated data for a single asset across all accounts.
struct AssetAggregate {
    total_amount: Decimal,
    latest_balance_date: NaiveDate,
    holdings: Vec<AssetHolding>,
}

/// Context loaded from storage for portfolio calculation.
struct CalculationContext {
    account_map: HashMap<Id, Account>,
    connection_map: HashMap<Id, Connection>,
    filtered_snapshots: Vec<(Id, BalanceSnapshot)>,
    zero_accounts: Vec<Id>,
}

impl PortfolioService {
    pub fn new(storage: Arc<dyn Storage>, market_data: Arc<MarketDataService>) -> Self {
        Self {
            storage,
            market_data,
            clock: Arc::new(SystemClock),
        }
    }

    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub async fn calculate(&self, query: &PortfolioQuery) -> Result<PortfolioSnapshot> {
        // Load accounts, connections, and balances
        let ctx = self.load_calculation_context(query.as_of_date).await?;

        // Aggregate balances by asset
        let by_asset_agg = Self::aggregate_by_asset(&ctx.filtered_snapshots)?;

        // Fetch valuations for all unique assets (cached)
        let price_cache = self
            .fetch_asset_valuations(&by_asset_agg, &query.currency, query.as_of_date)
            .await?;

        // Build asset summaries and calculate total value
        let (mut asset_summaries, total_value) = self.build_asset_summaries(
            &by_asset_agg,
            &price_cache,
            &ctx.account_map,
            query.include_detail,
            query.currency_decimals,
        )?;

        // Build account summaries
        let mut account_summaries = Self::build_account_summaries(
            &ctx.filtered_snapshots,
            &ctx.zero_accounts,
            &price_cache,
            &ctx.account_map,
            &ctx.connection_map,
            query.currency_decimals,
        )?;

        // Sort for consistent output
        account_summaries.sort_by(|a, b| a.account_name.cmp(&b.account_name));
        asset_summaries.sort_by(|a, b| {
            let a_id = AssetId::from_asset(&a.asset);
            let b_id = AssetId::from_asset(&b.asset);
            a_id.as_str().cmp(b_id.as_str())
        });

        // Build snapshot based on grouping
        let (by_asset, by_account) = match query.grouping {
            Grouping::Asset => (Some(asset_summaries), None),
            Grouping::Account => (None, Some(account_summaries)),
            Grouping::Both => (Some(asset_summaries), Some(account_summaries)),
        };

        Ok(PortfolioSnapshot {
            as_of_date: query.as_of_date,
            currency: query.currency.clone(),
            total_value: format_base_currency_value(total_value, query.currency_decimals),
            by_asset,
            by_account,
        })
    }

    /// Load accounts, connections, and balances from storage.
    async fn load_calculation_context(&self, as_of_date: NaiveDate) -> Result<CalculationContext> {
        let accounts = self.storage.list_accounts().await?;
        let connections = self.storage.list_connections().await?;

        let account_map: HashMap<Id, Account> =
            accounts.into_iter().map(|a| (a.id.clone(), a)).collect();
        let connection_map: HashMap<Id, Connection> = connections
            .into_iter()
            .map(|c| (c.id().clone(), c))
            .collect();

        let as_of_datetime = as_of_date.and_hms_opt(23, 59, 59).unwrap().and_utc();
        let mut filtered_snapshots = Vec::new();
        let mut zero_accounts = Vec::new();

        for account in account_map.values() {
            let snapshots = self.storage.get_balance_snapshots(&account.id).await?;
            if snapshots.is_empty() {
                let policy = self
                    .storage
                    .get_account_config(&account.id)?
                    .and_then(|config| config.balance_backfill)
                    .unwrap_or(BalanceBackfillPolicy::None);
                if matches!(policy, BalanceBackfillPolicy::Zero) {
                    zero_accounts.push(account.id.clone());
                }
                continue;
            }

            let latest_before = snapshots
                .iter()
                .filter(|s| s.timestamp <= as_of_datetime)
                .max_by_key(|s| s.timestamp)
                .cloned();

            if let Some(snapshot) = latest_before {
                filtered_snapshots.push((account.id.clone(), snapshot));
                continue;
            }

            let policy = self
                .storage
                .get_account_config(&account.id)?
                .and_then(|config| config.balance_backfill)
                .unwrap_or(BalanceBackfillPolicy::None);

            match policy {
                BalanceBackfillPolicy::CarryEarliest => {
                    if let Some(earliest) = snapshots.iter().min_by_key(|s| s.timestamp).cloned() {
                        filtered_snapshots.push((account.id.clone(), earliest));
                    }
                }
                BalanceBackfillPolicy::Zero => {
                    zero_accounts.push(account.id.clone());
                }
                BalanceBackfillPolicy::None => {}
            }
        }

        Ok(CalculationContext {
            account_map,
            connection_map,
            filtered_snapshots,
            zero_accounts,
        })
    }

    /// Aggregate balances by asset, tracking totals and holdings.
    fn aggregate_by_asset(
        snapshots: &[(Id, BalanceSnapshot)],
    ) -> Result<HashMap<Asset, AssetAggregate>> {
        let mut by_asset: HashMap<Asset, AssetAggregate> = HashMap::new();

        for (account_id, snapshot) in snapshots {
            for asset_balance in &snapshot.balances {
                let asset_key = asset_balance.asset.normalized();
                let amount = Decimal::from_str(&asset_balance.amount)?;
                let balance_date = snapshot.timestamp.date_naive();

                let entry = by_asset
                    .entry(asset_key.clone())
                    .or_insert_with(|| AssetAggregate {
                        total_amount: Decimal::ZERO,
                        latest_balance_date: balance_date,
                        holdings: Vec::new(),
                    });

                entry.total_amount += amount;
                if balance_date > entry.latest_balance_date {
                    entry.latest_balance_date = balance_date;
                }
                entry.holdings.push(AssetHolding {
                    account_id: account_id.clone(),
                    asset: asset_key.clone(),
                    amount: asset_balance.amount.clone(),
                    timestamp: snapshot.timestamp,
                });
            }
        }

        Ok(by_asset)
    }

    /// Fetch valuations for all unique assets, caching to avoid duplicate API calls.
    async fn fetch_asset_valuations(
        &self,
        by_asset: &HashMap<Asset, AssetAggregate>,
        target_currency: &str,
        as_of_date: NaiveDate,
    ) -> Result<HashMap<Asset, AssetValuation>> {
        let mut cache = HashMap::new();

        for asset in by_asset.keys() {
            let valuation = self
                .value_asset(asset, Decimal::ONE, target_currency, as_of_date)
                .await?;
            cache.insert(asset.clone(), valuation);
        }

        Ok(cache)
    }

    /// Build asset summaries from aggregated data and cached valuations.
    fn build_asset_summaries(
        &self,
        by_asset: &HashMap<Asset, AssetAggregate>,
        price_cache: &HashMap<Asset, AssetValuation>,
        account_map: &HashMap<Id, Account>,
        include_detail: bool,
        currency_decimals: Option<u32>,
    ) -> Result<(Vec<AssetSummary>, Decimal)> {
        let mut summaries = Vec::new();
        let mut total_value = Decimal::ZERO;

        for (asset, agg) in by_asset {
            let valuation = price_cache.get(asset).with_context(|| {
                format!("missing valuation for asset {}", AssetId::from_asset(asset))
            })?;

            let asset_value = valuation
                .value
                .map(|unit_price| unit_price * agg.total_amount);
            if let Some(v) = asset_value {
                total_value += v;
            }

            let holdings_detail = if include_detail {
                Some(Self::build_holdings_detail(&agg.holdings, account_map)?)
            } else {
                None
            };

            summaries.push(AssetSummary {
                asset: asset.clone(),
                total_amount: agg.total_amount.normalize().to_string(),
                amount_date: agg.latest_balance_date,
                price: valuation.price.clone(),
                price_date: valuation.price_date,
                price_timestamp: valuation.price_timestamp,
                fx_rate: valuation.fx_rate.clone(),
                fx_date: valuation.fx_date,
                value_in_base: asset_value
                    .map(|v| format_base_currency_value(v, currency_decimals)),
                holdings: holdings_detail,
            });
        }

        Ok((summaries, total_value))
    }

    /// Build holdings detail for an asset.
    fn build_holdings_detail(
        holdings: &[AssetHolding],
        account_map: &HashMap<Id, Account>,
    ) -> Result<Vec<AccountHolding>> {
        let mut detail = Vec::new();

        for holding in holdings {
            let account_name = account_map
                .get(&holding.account_id)
                .map(|a| a.name.clone())
                .unwrap_or_default();

            detail.push(AccountHolding {
                account_id: holding.account_id.to_string(),
                account_name,
                amount: Decimal::from_str(&holding.amount)?.normalize().to_string(),
                balance_date: holding.timestamp.date_naive(),
            });
        }

        Ok(detail)
    }

    /// Build account summaries by aggregating values across assets.
    fn build_account_summaries(
        snapshots: &[(Id, BalanceSnapshot)],
        zero_accounts: &[Id],
        price_cache: &HashMap<Asset, AssetValuation>,
        account_map: &HashMap<Id, Account>,
        connection_map: &HashMap<Id, Connection>,
        currency_decimals: Option<u32>,
    ) -> Result<Vec<AccountSummary>> {
        // Track (sum, has_missing_values) per account
        let mut by_account: HashMap<Id, (Decimal, bool)> = HashMap::new();

        for (account_id, snapshot) in snapshots {
            for asset_balance in &snapshot.balances {
                let asset_key = asset_balance.asset.normalized();
                let amount = Decimal::from_str(&asset_balance.amount)?;
                let valuation = price_cache.get(&asset_key).with_context(|| {
                    format!(
                        "missing valuation for asset {}",
                        AssetId::from_asset(&asset_key)
                    )
                })?;

                let entry = by_account
                    .entry(account_id.clone())
                    .or_insert((Decimal::ZERO, false));

                match valuation.value {
                    Some(unit_price) => entry.0 += unit_price * amount,
                    None => entry.1 = true,
                }
            }
        }

        let mut summaries: Vec<AccountSummary> = by_account
            .into_iter()
            .filter_map(|(account_id, (value, has_missing))| {
                let account = account_map.get(&account_id)?;
                let connection = connection_map.get(&account.connection_id)?;
                Some(AccountSummary {
                    account_id: account_id.to_string(),
                    account_name: account.name.clone(),
                    connection_name: connection.name().to_string(),
                    value_in_base: if has_missing {
                        None
                    } else {
                        Some(format_base_currency_value(value, currency_decimals))
                    },
                })
            })
            .collect();

        for account_id in zero_accounts {
            if summaries
                .iter()
                .any(|s| s.account_id == account_id.to_string())
            {
                continue;
            }
            let account = match account_map.get(account_id) {
                Some(account) => account,
                None => continue,
            };
            let connection = match connection_map.get(&account.connection_id) {
                Some(connection) => connection,
                None => continue,
            };
            summaries.push(AccountSummary {
                account_id: account_id.to_string(),
                account_name: account.name.clone(),
                connection_name: connection.name().to_string(),
                value_in_base: Some(format_base_currency_value(Decimal::ZERO, currency_decimals)),
            });
        }

        Ok(summaries)
    }

    /// Value an asset in the target currency.
    /// Uses live quotes when available, falls back to historical close prices.
    async fn value_asset(
        &self,
        asset: &Asset,
        amount: Decimal,
        target_currency: &str,
        as_of_date: NaiveDate,
    ) -> Result<AssetValuation> {
        match asset {
            Asset::Currency { iso_code } => {
                if iso_code.eq_ignore_ascii_case(target_currency) {
                    // Same currency, no conversion needed
                    Ok(AssetValuation {
                        value: Some(amount),
                        price: None,
                        price_date: None,
                        price_timestamp: None,
                        fx_rate: None,
                        fx_date: None,
                    })
                } else {
                    // Need FX conversion
                    match self
                        .market_data
                        .fx_close(iso_code, target_currency, as_of_date)
                        .await
                    {
                        Ok(rate) => {
                            let fx_rate = Decimal::from_str(&rate.rate)?;
                            Ok(AssetValuation {
                                value: Some(amount * fx_rate),
                                price: None,
                                price_date: None,
                                price_timestamp: None,
                                fx_rate: Some(fx_rate.normalize().to_string()),
                                fx_date: Some(rate.as_of_date),
                            })
                        }
                        Err(_) => {
                            // No FX rate available
                            Ok(AssetValuation {
                                value: None,
                                price: None,
                                price_date: None,
                                price_timestamp: None,
                                fx_rate: None,
                                fx_date: None,
                            })
                        }
                    }
                }
            }
            Asset::Equity { .. } | Asset::Crypto { .. } => {
                // Get price - try live quote first, fall back to close
                let price_result = if as_of_date == self.clock.today() {
                    self.market_data.price_latest(asset, as_of_date).await
                } else {
                    self.market_data.price_close(asset, as_of_date).await
                };
                let price_point = match price_result {
                    Ok(p) => p,
                    Err(_) => {
                        // No price available
                        return Ok(AssetValuation {
                            value: None,
                            price: None,
                            price_date: None,
                            price_timestamp: None,
                            fx_rate: None,
                            fx_date: None,
                        });
                    }
                };
                let price = Decimal::from_str(&price_point.price)?;
                let value_in_quote = amount * price;

                // Convert to target currency if needed
                if price_point
                    .quote_currency
                    .eq_ignore_ascii_case(target_currency)
                {
                    Ok(AssetValuation {
                        value: Some(value_in_quote),
                        price: Some(price.normalize().to_string()),
                        price_date: Some(price_point.as_of_date),
                        price_timestamp: Some(price_point.timestamp),
                        fx_rate: None,
                        fx_date: None,
                    })
                } else {
                    match self
                        .market_data
                        .fx_close(&price_point.quote_currency, target_currency, as_of_date)
                        .await
                    {
                        Ok(rate) => {
                            let fx_rate = Decimal::from_str(&rate.rate)?;
                            Ok(AssetValuation {
                                value: Some(value_in_quote * fx_rate),
                                price: Some(price.normalize().to_string()),
                                price_date: Some(price_point.as_of_date),
                                price_timestamp: Some(price_point.timestamp),
                                fx_rate: Some(fx_rate.normalize().to_string()),
                                fx_date: Some(rate.as_of_date),
                            })
                        }
                        Err(_) => {
                            // Have price but no FX rate
                            Ok(AssetValuation {
                                value: None,
                                price: Some(price.normalize().to_string()),
                                price_date: Some(price_point.as_of_date),
                                price_timestamp: Some(price_point.timestamp),
                                fx_rate: None,
                                fx_date: None,
                            })
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Grouping;
    use super::*;
    use crate::market_data::{
        AssetId, EquityPriceRouter, EquityPriceSource, PriceKind, PricePoint,
    };
    use crate::market_data::{MarketDataStore, MemoryMarketDataStore};
    use crate::models::{
        Account, AccountConfig, Asset, AssetBalance, BalanceBackfillPolicy, BalanceSnapshot,
        Connection, ConnectionConfig,
    };
    use crate::storage::MemoryStorage;
    use chrono::{TimeZone, Utc};
    use rust_decimal::Decimal;
    use std::sync::Arc;

    #[test]
    fn build_asset_summaries_errors_on_missing_price_cache_entry() {
        let storage = Arc::new(MemoryStorage::new());
        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = Arc::new(MarketDataService::new(store, None));
        let service = PortfolioService::new(storage, market_data);

        let asset = Asset::equity("AAPL");
        let mut by_asset: std::collections::HashMap<Asset, super::AssetAggregate> =
            std::collections::HashMap::new();
        by_asset.insert(
            asset.clone(),
            super::AssetAggregate {
                total_amount: Decimal::ONE,
                latest_balance_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
                holdings: Vec::new(),
            },
        );

        let price_cache: std::collections::HashMap<Asset, super::AssetValuation> =
            std::collections::HashMap::new();
        let account_map: std::collections::HashMap<Id, Account> = std::collections::HashMap::new();

        let err = service
            .build_asset_summaries(&by_asset, &price_cache, &account_map, false, None)
            .unwrap_err();
        assert!(err.to_string().contains("missing valuation"));
    }

    #[test]
    fn build_account_summaries_errors_on_missing_price_cache_entry() {
        let asset = Asset::equity("AAPL");
        let snapshot = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
            vec![AssetBalance::new(asset.clone(), "1")],
        );

        let snapshots = vec![(Id::from_string("acct-1"), snapshot)];
        let price_cache: std::collections::HashMap<Asset, super::AssetValuation> =
            std::collections::HashMap::new();
        let account_map: std::collections::HashMap<Id, Account> = std::collections::HashMap::new();
        let connection_map: std::collections::HashMap<Id, Connection> =
            std::collections::HashMap::new();

        let err = PortfolioService::build_account_summaries(
            &snapshots,
            &[],
            &price_cache,
            &account_map,
            &connection_map,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("missing valuation"));
    }

    #[tokio::test]
    async fn calculate_single_currency_holding() -> Result<()> {
        // Setup storage with one account holding USD
        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(ConnectionConfig {
            name: "Test Bank".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        });
        storage.save_connection(&connection).await?;

        let account = Account::new("Checking", connection.id().clone());
        storage.save_account(&account).await?;

        let snapshot = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
            vec![AssetBalance::new(Asset::currency("USD"), "1000.00")],
        );
        storage
            .append_balance_snapshot(&account.id, &snapshot)
            .await?;

        // Setup market data (no prices needed for USD->USD)
        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = Arc::new(MarketDataService::new(store, None));

        // Calculate
        let service = PortfolioService::new(storage, market_data);
        let query = PortfolioQuery {
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
            currency: "USD".to_string(),
            currency_decimals: None,
            grouping: Grouping::Both,
            include_detail: false,
        };
        let result = service.calculate(&query).await?;

        // Decimal::normalize() removes trailing zeros, so "1000.00" becomes "1000"
        assert_eq!(result.total_value, "1000");
        assert_eq!(result.currency, "USD");
        Ok(())
    }

    #[tokio::test]
    async fn calculate_with_equity_and_fx() -> Result<()> {
        use crate::market_data::{AssetId, FxRateKind, FxRatePoint, PriceKind, PricePoint};

        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(ConnectionConfig {
            name: "Broker".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        });
        storage.save_connection(&connection).await?;

        let account = Account::new("Brokerage", connection.id().clone());
        storage.save_account(&account).await?;

        // 10 shares of AAPL
        let snapshot = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
            vec![AssetBalance::new(Asset::equity("AAPL"), "10")],
        );
        storage
            .append_balance_snapshot(&account.id, &snapshot)
            .await?;

        // Setup market data with AAPL at $200 and USD/EUR at 0.91
        let store = Arc::new(MemoryMarketDataStore::new());
        let as_of_date = chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap();

        // Store AAPL price
        let aapl_price = PricePoint {
            asset_id: AssetId::from_asset(&Asset::equity("AAPL")),
            as_of_date,
            timestamp: Utc::now(),
            price: "200".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        };
        store.put_prices(&[aapl_price]).await?;

        // Store USD->EUR FX rate
        let fx_rate = FxRatePoint {
            base: "USD".to_string(),
            quote: "EUR".to_string(),
            as_of_date,
            timestamp: Utc::now(),
            rate: "0.91".to_string(),
            kind: FxRateKind::Close,
            source: "test".to_string(),
        };
        store.put_fx_rates(&[fx_rate]).await?;

        let market_data = Arc::new(MarketDataService::new(store, None));

        // Calculate in EUR
        let service = PortfolioService::new(storage, market_data);
        let query = PortfolioQuery {
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
            currency: "EUR".to_string(),
            currency_decimals: None,
            grouping: Grouping::Asset,
            include_detail: false,
        };
        let result = service.calculate(&query).await?;

        // 10 shares * $200 = $2000 * 0.91 = 1820 EUR
        assert_eq!(result.total_value, "1820");
        assert_eq!(result.currency, "EUR");

        // Check asset summary
        let by_asset = result.by_asset.unwrap();
        assert_eq!(by_asset.len(), 1);
        assert_eq!(by_asset[0].total_amount, "10");
        assert_eq!(by_asset[0].price, Some("200".to_string()));
        assert_eq!(by_asset[0].fx_rate, Some("0.91".to_string()));
        assert_eq!(by_asset[0].value_in_base, Some("1820".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn calculate_with_detail() -> Result<()> {
        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(ConnectionConfig {
            name: "Bank".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        });
        storage.save_connection(&connection).await?;

        // Create two accounts
        let account1 = Account::new("Checking", connection.id().clone());
        let account2 = Account::new("Savings", connection.id().clone());
        storage.save_account(&account1).await?;
        storage.save_account(&account2).await?;

        // Add USD balances to both accounts
        let snapshot1 = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
            vec![AssetBalance::new(Asset::currency("USD"), "1000")],
        );
        let snapshot2 = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2026, 2, 1, 14, 0, 0).unwrap(),
            vec![AssetBalance::new(Asset::currency("USD"), "2000")],
        );
        storage
            .append_balance_snapshot(&account1.id, &snapshot1)
            .await?;
        storage
            .append_balance_snapshot(&account2.id, &snapshot2)
            .await?;

        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = Arc::new(MarketDataService::new(store, None));

        let service = PortfolioService::new(storage, market_data);
        let query = PortfolioQuery {
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
            currency: "USD".to_string(),
            currency_decimals: None,
            grouping: Grouping::Asset,
            include_detail: true,
        };
        let result = service.calculate(&query).await?;

        // Total should be 3000
        assert_eq!(result.total_value, "3000");

        // Check asset summary with holdings detail
        let by_asset = result.by_asset.unwrap();
        assert_eq!(by_asset.len(), 1);
        assert_eq!(by_asset[0].total_amount, "3000");

        // Check holdings detail
        let holdings = by_asset[0].holdings.as_ref().unwrap();
        assert_eq!(holdings.len(), 2);

        // Find the checking and savings holdings
        let checking_holding = holdings.iter().find(|h| h.account_name == "Checking");
        let savings_holding = holdings.iter().find(|h| h.account_name == "Savings");

        assert!(checking_holding.is_some());
        assert!(savings_holding.is_some());
        assert_eq!(checking_holding.unwrap().amount, "1000");
        assert_eq!(savings_holding.unwrap().amount, "2000");

        Ok(())
    }

    #[tokio::test]
    async fn calculate_merges_case_insensitive_assets() -> Result<()> {
        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(ConnectionConfig {
            name: "Bank".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        });
        storage.save_connection(&connection).await?;

        let account1 = Account::new("Checking", connection.id().clone());
        let account2 = Account::new("Savings", connection.id().clone());
        storage.save_account(&account1).await?;
        storage.save_account(&account2).await?;

        let snapshot1 = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
            vec![AssetBalance::new(Asset::currency("USD"), "1000")],
        );
        let snapshot2 = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2026, 2, 1, 14, 0, 0).unwrap(),
            vec![AssetBalance::new(Asset::currency(" usd "), "2000")],
        );
        storage
            .append_balance_snapshot(&account1.id, &snapshot1)
            .await?;
        storage
            .append_balance_snapshot(&account2.id, &snapshot2)
            .await?;

        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = Arc::new(MarketDataService::new(store, None));
        let service = PortfolioService::new(storage, market_data);

        let query = PortfolioQuery {
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
            currency: "USD".to_string(),
            currency_decimals: None,
            grouping: Grouping::Asset,
            include_detail: false,
        };
        let result = service.calculate(&query).await?;

        let by_asset = result.by_asset.unwrap();
        assert_eq!(by_asset.len(), 1);
        assert_eq!(by_asset[0].total_amount, "3000");
        match &by_asset[0].asset {
            Asset::Currency { iso_code } => assert_eq!(iso_code, "USD"),
            _ => panic!("expected currency asset"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn calculate_uses_latest_snapshot_before_date() -> Result<()> {
        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(ConnectionConfig {
            name: "Test Bank".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        });
        storage.save_connection(&connection).await?;

        let account = Account::new("Checking", connection.id().clone());
        storage.save_account(&account).await?;

        let older_snapshot = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap(),
            vec![AssetBalance::new(Asset::currency("USD"), "1000")],
        );
        let newer_snapshot = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2026, 2, 3, 12, 0, 0).unwrap(),
            vec![AssetBalance::new(Asset::currency("USD"), "2000")],
        );
        storage
            .append_balance_snapshot(&account.id, &older_snapshot)
            .await?;
        storage
            .append_balance_snapshot(&account.id, &newer_snapshot)
            .await?;

        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = Arc::new(MarketDataService::new(store, None));
        let service = PortfolioService::new(storage, market_data);

        let query = PortfolioQuery {
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
            currency: "USD".to_string(),
            currency_decimals: None,
            grouping: Grouping::Both,
            include_detail: false,
        };

        let result = service.calculate(&query).await?;
        assert_eq!(result.total_value, "1000");
        Ok(())
    }

    #[tokio::test]
    async fn calculate_zero_backfill() -> Result<()> {
        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(ConnectionConfig {
            name: "Test Bank".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        });
        storage.save_connection(&connection).await?;

        let account = Account::new("Checking", connection.id().clone());
        storage.save_account(&account).await?;
        storage
            .set_account_config(
                &account.id,
                AccountConfig {
                    balance_backfill: Some(BalanceBackfillPolicy::Zero),
                    ..AccountConfig::default()
                },
            )
            .await;

        let future_snapshot = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2026, 2, 3, 12, 0, 0).unwrap(),
            vec![AssetBalance::new(Asset::currency("USD"), "1000")],
        );
        storage
            .append_balance_snapshot(&account.id, &future_snapshot)
            .await?;

        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = Arc::new(MarketDataService::new(store, None));
        let service = PortfolioService::new(storage, market_data);

        let query = PortfolioQuery {
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            currency: "USD".to_string(),
            currency_decimals: None,
            grouping: Grouping::Account,
            include_detail: false,
        };

        let result = service.calculate(&query).await?;
        assert_eq!(result.total_value, "0");

        let by_account = result.by_account.expect("account summaries");
        assert_eq!(by_account.len(), 1);
        assert_eq!(by_account[0].value_in_base.as_deref(), Some("0"));
        Ok(())
    }

    #[tokio::test]
    async fn calculate_carry_back_earliest_balance() -> Result<()> {
        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(ConnectionConfig {
            name: "Test Bank".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
            balance_staleness: None,
        });
        storage.save_connection(&connection).await?;

        let account = Account::new("Checking", connection.id().clone());
        storage.save_account(&account).await?;
        storage
            .set_account_config(
                &account.id,
                AccountConfig {
                    balance_backfill: Some(BalanceBackfillPolicy::CarryEarliest),
                    ..AccountConfig::default()
                },
            )
            .await;

        let earliest_snapshot = BalanceSnapshot::new(
            Utc.with_ymd_and_hms(2026, 2, 3, 12, 0, 0).unwrap(),
            vec![AssetBalance::new(Asset::currency("USD"), "1000")],
        );
        storage
            .append_balance_snapshot(&account.id, &earliest_snapshot)
            .await?;

        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = Arc::new(MarketDataService::new(store, None));
        let service = PortfolioService::new(storage, market_data);

        let query = PortfolioQuery {
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            currency: "USD".to_string(),
            currency_decimals: None,
            grouping: Grouping::Both,
            include_detail: false,
        };

        let result = service.calculate(&query).await?;
        assert_eq!(result.total_value, "1000");
        Ok(())
    }

    #[tokio::test]
    async fn historical_snapshot_uses_close_not_live_quote() -> Result<()> {
        #[derive(Clone)]
        struct QuoteOnlySource {
            quote: PricePoint,
        }

        #[async_trait::async_trait]
        impl EquityPriceSource for QuoteOnlySource {
            async fn fetch_close(
                &self,
                _asset: &Asset,
                _asset_id: &AssetId,
                _date: chrono::NaiveDate,
            ) -> Result<Option<PricePoint>> {
                Ok(None)
            }

            async fn fetch_quote(
                &self,
                _asset: &Asset,
                _asset_id: &AssetId,
            ) -> Result<Option<PricePoint>> {
                Ok(Some(self.quote.clone()))
            }

            fn name(&self) -> &str {
                "quote-only"
            }
        }

        let asset = Asset::equity("AAPL");
        let asset_id = AssetId::from_asset(&asset);
        let as_of_date = chrono::NaiveDate::from_ymd_opt(2024, 1, 2).unwrap();

        let store = Arc::new(MemoryMarketDataStore::new());
        let close_price = PricePoint {
            asset_id: asset_id.clone(),
            as_of_date,
            timestamp: Utc::now(),
            price: "100".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "close".to_string(),
        };
        store.put_prices(&[close_price]).await?;

        let quote_price = PricePoint {
            asset_id,
            as_of_date: Utc::now().date_naive(),
            timestamp: Utc::now(),
            price: "200".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Quote,
            source: "quote".to_string(),
        };

        let router = EquityPriceRouter::new(vec![Arc::new(QuoteOnlySource { quote: quote_price })]);
        let market_data =
            Arc::new(MarketDataService::new(store, None).with_equity_router(Arc::new(router)));

        let service = PortfolioService::new(Arc::new(MemoryStorage::new()), market_data);
        let valuation = service
            .value_asset(&asset, Decimal::ONE, "USD", as_of_date)
            .await?;

        assert_eq!(valuation.price.as_deref(), Some("100"));
        assert_eq!(valuation.price_date, Some(as_of_date));

        Ok(())
    }
}

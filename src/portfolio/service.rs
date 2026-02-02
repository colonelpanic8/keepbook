// src/portfolio/service.rs
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::market_data::MarketDataService;
use crate::models::{Account, Asset, Balance, Connection, Id};
use crate::storage::Storage;

use super::{
    AccountHolding, AccountSummary, AssetSummary, Grouping, PortfolioQuery, PortfolioSnapshot,
    RefreshPolicy,
};

pub struct PortfolioService {
    storage: Arc<dyn Storage>,
    market_data: Arc<MarketDataService>,
}

/// Valuation result for an asset.
struct AssetValuation {
    /// The value in target currency. None if price data unavailable.
    value: Option<Decimal>,
    price: Option<String>,
    price_date: Option<NaiveDate>,
    fx_rate: Option<String>,
    fx_date: Option<NaiveDate>,
}

impl PortfolioService {
    pub fn new(storage: Arc<dyn Storage>, market_data: Arc<MarketDataService>) -> Self {
        Self {
            storage,
            market_data,
        }
    }

    pub async fn calculate(
        &self,
        query: &PortfolioQuery,
        _refresh: &RefreshPolicy,
    ) -> Result<PortfolioSnapshot> {
        // 1. Get all accounts and connections
        let accounts = self.storage.list_accounts().await?;
        let connections = self.storage.list_connections().await?;

        // Build lookup maps
        let account_map: HashMap<Id, Account> =
            accounts.iter().cloned().map(|a| (a.id.clone(), a)).collect();
        let connection_map: HashMap<Id, Connection> = connections
            .iter()
            .cloned()
            .map(|c| (c.id().clone(), c))
            .collect();

        // 2. Get latest balances, filtered by as_of_date
        let all_balances = self.storage.get_latest_balances().await?;
        let as_of_datetime = query
            .as_of_date
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc();

        let filtered_balances: Vec<(Id, Balance)> = all_balances
            .into_iter()
            .filter(|(_, balance)| balance.timestamp <= as_of_datetime)
            .collect();

        // 3. Aggregate by asset (for by_asset summary)
        // Key: serialized asset, Value: (total amount, latest balance date, list of (account_id, balance))
        let mut by_asset_agg: HashMap<String, (Decimal, NaiveDate, Vec<(Id, Balance)>)> =
            HashMap::new();
        for (account_id, balance) in &filtered_balances {
            let asset_key = serde_json::to_string(&balance.asset)?;
            let amount = Decimal::from_str(&balance.amount)?;
            let balance_date = balance.timestamp.date_naive();
            let entry = by_asset_agg
                .entry(asset_key)
                .or_insert((Decimal::ZERO, balance_date, Vec::new()));
            entry.0 += amount;
            // Track the most recent balance date
            if balance_date > entry.1 {
                entry.1 = balance_date;
            }
            entry.2.push((account_id.clone(), balance.clone()));
        }

        // 4. Calculate values for each asset
        let mut asset_summaries = Vec::new();
        let mut total_value = Decimal::ZERO;

        for (asset_key, (total_amount, latest_balance_date, holdings)) in &by_asset_agg {
            let asset: Asset = serde_json::from_str(asset_key)?;
            let valuation = self
                .value_asset(&asset, *total_amount, &query.currency, query.as_of_date)
                .await?;

            // Only add to total if we have a value
            if let Some(v) = valuation.value {
                total_value += v;
            }

            // Build holdings detail if requested
            let holdings_detail = if query.include_detail {
                let mut detail = Vec::new();
                for (account_id, balance) in holdings {
                    let account = account_map.get(account_id);
                    let account_name = account.map(|a| a.name.clone()).unwrap_or_default();
                    detail.push(AccountHolding {
                        account_id: account_id.to_string(),
                        account_name,
                        amount: Decimal::from_str(&balance.amount)?
                            .normalize()
                            .to_string(),
                        balance_date: balance.timestamp.date_naive(),
                    });
                }
                Some(detail)
            } else {
                None
            };

            asset_summaries.push(AssetSummary {
                asset: asset.clone(),
                total_amount: total_amount.normalize().to_string(),
                amount_date: *latest_balance_date,
                price: valuation.price,
                price_date: valuation.price_date,
                fx_rate: valuation.fx_rate,
                fx_date: valuation.fx_date,
                value_in_base: valuation.value.map(|v| v.normalize().to_string()),
                holdings: holdings_detail,
            });
        }

        // 5. Aggregate by account (for by_account summary)
        // Track (sum, has_missing_values) per account
        let mut by_account_values: HashMap<Id, (Decimal, bool)> = HashMap::new();
        for (account_id, balance) in &filtered_balances {
            let amount = Decimal::from_str(&balance.amount)?;
            let valuation = self
                .value_asset(&balance.asset, amount, &query.currency, query.as_of_date)
                .await?;
            let entry = by_account_values
                .entry(account_id.clone())
                .or_insert((Decimal::ZERO, false));
            match valuation.value {
                Some(v) => entry.0 += v,
                None => entry.1 = true, // Mark as having missing values
            }
        }

        let mut account_summaries: Vec<AccountSummary> = by_account_values
            .into_iter()
            .filter_map(|(account_id, (value, has_missing))| {
                let account = account_map.get(&account_id)?;
                let connection = connection_map.get(&account.connection_id)?;
                Some(AccountSummary {
                    account_id: account_id.to_string(),
                    account_name: account.name.clone(),
                    connection_name: connection.name().to_string(),
                    // Show value only if no assets are missing prices
                    value_in_base: if has_missing {
                        None
                    } else {
                        Some(value.normalize().to_string())
                    },
                })
            })
            .collect();

        // Sort for consistent output
        account_summaries.sort_by(|a, b| a.account_name.cmp(&b.account_name));
        asset_summaries.sort_by(|a, b| {
            serde_json::to_string(&a.asset)
                .unwrap_or_default()
                .cmp(&serde_json::to_string(&b.asset).unwrap_or_default())
        });

        // 6. Build snapshot based on grouping
        let (by_asset, by_account) = match query.grouping {
            Grouping::Asset => (Some(asset_summaries), None),
            Grouping::Account => (None, Some(account_summaries)),
            Grouping::Both => (Some(asset_summaries), Some(account_summaries)),
        };

        Ok(PortfolioSnapshot {
            as_of_date: query.as_of_date,
            currency: query.currency.clone(),
            total_value: total_value.normalize().to_string(),
            by_asset,
            by_account,
        })
    }

    /// Value an asset in the target currency.
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
                                fx_rate: None,
                                fx_date: None,
                            })
                        }
                    }
                }
            }
            Asset::Equity { .. } | Asset::Crypto { .. } => {
                // Get price - return None value if price unavailable
                let price_result = self.market_data.price_close(asset, as_of_date).await;
                let price_point = match price_result {
                    Ok(p) => p,
                    Err(_) => {
                        // No price available
                        return Ok(AssetValuation {
                            value: None,
                            price: None,
                            price_date: None,
                            fx_rate: None,
                            fx_date: None,
                        });
                    }
                };
                let price = Decimal::from_str(&price_point.price)?;
                let value_in_quote = amount * price;

                // Convert to target currency if needed
                if price_point.quote_currency.eq_ignore_ascii_case(target_currency) {
                    Ok(AssetValuation {
                        value: Some(value_in_quote),
                        price: Some(price.normalize().to_string()),
                        price_date: Some(price_point.as_of_date),
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
    use super::*;
    use crate::market_data::{MarketDataStore, MemoryMarketDataStore};
    use crate::models::{Account, Asset, Balance, ConnectionConfig, Connection};
    use crate::storage::MemoryStorage;
    use chrono::{TimeZone, Utc};
    use super::super::Grouping;

    #[tokio::test]
    async fn calculate_single_currency_holding() -> Result<()> {
        // Setup storage with one account holding USD
        let storage = Arc::new(MemoryStorage::new());
        let connection = Connection::new(ConnectionConfig {
            name: "Test Bank".to_string(),
            synchronizer: "manual".to_string(),
            credentials: None,
        });
        storage.save_connection(&connection).await?;

        let account = Account::new("Checking", connection.id().clone());
        storage.save_account(&account).await?;

        let balance = Balance::new(Asset::currency("USD"), "1000.00")
            .with_timestamp(Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap());
        storage.save_balance(&account.id, &balance).await?;

        // Setup market data (no prices needed for USD->USD)
        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = Arc::new(MarketDataService::new(store, None));

        // Calculate
        let service = PortfolioService::new(storage, market_data);
        let query = PortfolioQuery {
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
            currency: "USD".to_string(),
            grouping: Grouping::Both,
            include_detail: false,
        };
        let result = service.calculate(&query, &RefreshPolicy::default()).await?;

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
        });
        storage.save_connection(&connection).await?;

        let account = Account::new("Brokerage", connection.id().clone());
        storage.save_account(&account).await?;

        // 10 shares of AAPL
        let balance = Balance::new(Asset::equity("AAPL"), "10")
            .with_timestamp(Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap());
        storage.save_balance(&account.id, &balance).await?;

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
            grouping: Grouping::Asset,
            include_detail: false,
        };
        let result = service.calculate(&query, &RefreshPolicy::default()).await?;

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
        });
        storage.save_connection(&connection).await?;

        // Create two accounts
        let account1 = Account::new("Checking", connection.id().clone());
        let account2 = Account::new("Savings", connection.id().clone());
        storage.save_account(&account1).await?;
        storage.save_account(&account2).await?;

        // Add USD balances to both accounts
        let balance1 = Balance::new(Asset::currency("USD"), "1000")
            .with_timestamp(Utc.with_ymd_and_hms(2026, 2, 1, 12, 0, 0).unwrap());
        let balance2 = Balance::new(Asset::currency("USD"), "2000")
            .with_timestamp(Utc.with_ymd_and_hms(2026, 2, 1, 14, 0, 0).unwrap());
        storage.save_balance(&account1.id, &balance1).await?;
        storage.save_balance(&account2.id, &balance2).await?;

        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = Arc::new(MarketDataService::new(store, None));

        let service = PortfolioService::new(storage, market_data);
        let query = PortfolioQuery {
            as_of_date: chrono::NaiveDate::from_ymd_opt(2026, 2, 2).unwrap(),
            currency: "USD".to_string(),
            grouping: Grouping::Asset,
            include_detail: true,
        };
        let result = service.calculate(&query, &RefreshPolicy::default()).await?;

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
}

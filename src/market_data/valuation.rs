use std::collections::HashMap;
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{FxRatePoint, MarketDataService, PricePoint};
use crate::models::{Asset, Balance, Id};
use crate::storage::Storage;

#[derive(Debug, Clone)]
pub struct AccountBalances {
    pub account_id: Id,
    pub balances: Vec<Balance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetWorthLineItem {
    pub account_id: Id,
    pub asset: Asset,
    pub amount: String,
    pub value: String,
    pub balance_timestamp: DateTime<Utc>,
    pub price: Option<PricePoint>,
    pub fx: Option<FxRatePoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountNetWorth {
    pub account_id: Id,
    pub total: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetWorthResult {
    pub as_of: DateTime<Utc>,
    pub base_currency: String,
    pub total: String,
    pub account_totals: Vec<AccountNetWorth>,
    pub line_items: Vec<NetWorthLineItem>,
}

pub struct NetWorthCalculator {
    market_data: MarketDataService,
}

impl NetWorthCalculator {
    pub fn new(market_data: MarketDataService) -> Self {
        Self { market_data }
    }

    pub async fn net_worth_from_storage(
        &self,
        storage: &dyn Storage,
        as_of: DateTime<Utc>,
        base_currency: &str,
    ) -> Result<NetWorthResult> {
        let accounts = storage.list_accounts().await?;
        let mut balances = Vec::new();

        for account in accounts {
            let account_balances = storage.get_balances(&account.id).await?;
            balances.push(AccountBalances {
                account_id: account.id,
                balances: account_balances,
            });
        }

        self.net_worth_for_accounts(&balances, as_of, base_currency)
            .await
    }

    pub async fn net_worth_for_accounts(
        &self,
        accounts: &[AccountBalances],
        as_of: DateTime<Utc>,
        base_currency: &str,
    ) -> Result<NetWorthResult> {
        let base_currency = normalize_currency_code(base_currency);
        let mut account_totals: HashMap<Id, Decimal> = HashMap::new();
        let mut line_items = Vec::new();

        for account in accounts {
            let latest_balances = latest_balances_as_of(&account.balances, as_of);
            for balance in latest_balances {
                let amount = parse_decimal(&balance.amount)?;
                let (value, price, fx) =
                    self.value_balance(&balance.asset, amount, &base_currency, as_of)
                        .await?;

                let total = account_totals
                    .entry(account.account_id.clone())
                    .or_insert_with(|| Decimal::ZERO);
                *total += value;

                line_items.push(NetWorthLineItem {
                    account_id: account.account_id.clone(),
                    asset: balance.asset.clone(),
                    amount: balance.amount.clone(),
                    value: decimal_to_string(value),
                    balance_timestamp: balance.timestamp,
                    price,
                    fx,
                });
            }
        }

        let mut total = Decimal::ZERO;
        let mut account_totals_vec = Vec::new();

        for (account_id, value) in account_totals {
            total += value;
            account_totals_vec.push(AccountNetWorth {
                account_id,
                total: decimal_to_string(value),
            });
        }

        Ok(NetWorthResult {
            as_of,
            base_currency,
            total: decimal_to_string(total),
            account_totals: account_totals_vec,
            line_items,
        })
    }

    async fn value_balance(
        &self,
        asset: &Asset,
        amount: Decimal,
        base_currency: &str,
        as_of: DateTime<Utc>,
    ) -> Result<(Decimal, Option<PricePoint>, Option<FxRatePoint>)> {
        let date = as_of.date_naive();

        match asset {
            Asset::Currency { iso_code } => {
                let iso_code = normalize_currency_code(iso_code);
                if iso_code == base_currency {
                    Ok((amount, None, None))
                } else {
                    let fx = self
                        .market_data
                        .fx_close(&iso_code, base_currency, date)
                        .await?;
                    let rate = parse_decimal(&fx.rate)?;
                    Ok((amount * rate, None, Some(fx)))
                }
            }
            _ => {
                let price = self.market_data.price_close(asset, date).await?;
                let price_value = parse_decimal(&price.price)?;
                let mut value = amount * price_value;
                let mut fx_point = None;

                let quote_currency = normalize_currency_code(&price.quote_currency);
                if quote_currency != base_currency {
                    let fx = self
                        .market_data
                        .fx_close(&quote_currency, base_currency, date)
                        .await?;
                    let rate = parse_decimal(&fx.rate)?;
                    value *= rate;
                    fx_point = Some(fx);
                }

                Ok((value, Some(price), fx_point))
            }
        }
    }
}

fn latest_balances_as_of(balances: &[Balance], as_of: DateTime<Utc>) -> Vec<Balance> {
    let mut latest: HashMap<Asset, Balance> = HashMap::new();

    for balance in balances.iter().filter(|b| b.timestamp <= as_of) {
        latest
            .entry(balance.asset.clone())
            .and_modify(|existing| {
                if balance.timestamp > existing.timestamp {
                    *existing = balance.clone();
                }
            })
            .or_insert_with(|| balance.clone());
    }

    latest.into_values().collect()
}

fn normalize_currency_code(value: &str) -> String {
    value.trim().to_uppercase()
}

fn parse_decimal(value: &str) -> Result<Decimal> {
    Decimal::from_str(value)
        .with_context(|| format!("Invalid decimal value: {}", value))
}

fn decimal_to_string(value: Decimal) -> String {
    value.normalize().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market_data::{AssetId, FxRateKind, MarketDataStore, MemoryMarketDataStore, PriceKind};
    use crate::models::Asset;
    use chrono::TimeZone;

    #[tokio::test]
    async fn net_worth_uses_prices_and_fx() -> Result<()> {
        let store = std::sync::Arc::new(MemoryMarketDataStore::new());
        let service = MarketDataService::new(store.clone(), None);
        let calculator = NetWorthCalculator::new(service);

        let asset = Asset::equity("AAPL");
        let asset_id = AssetId::from_asset(&asset);
        let date = Utc.with_ymd_and_hms(2024, 6, 3, 16, 0, 0).unwrap();

        let price = PricePoint {
            asset_id: asset_id.clone(),
            as_of_date: date.date_naive(),
            timestamp: date,
            price: "200".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        };

        let fx = FxRatePoint {
            base: "USD".to_string(),
            quote: "EUR".to_string(),
            as_of_date: date.date_naive(),
            timestamp: date,
            rate: "0.9".to_string(),
            kind: FxRateKind::Close,
            source: "test".to_string(),
        };

        store.put_prices(&[price]).await?;
        store.put_fx_rates(&[fx]).await?;

        let balances = vec![AccountBalances {
            account_id: Id::from("acct-1"),
            balances: vec![Balance {
                timestamp: date,
                asset,
                amount: "10".to_string(),
            }],
        }];

        let result = calculator
            .net_worth_for_accounts(&balances, date, "EUR")
            .await?;

        assert_eq!(result.total, "1800");
        Ok(())
    }
}

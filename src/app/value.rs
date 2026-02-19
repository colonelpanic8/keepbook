use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::format::format_base_currency_value;
use crate::market_data::MarketDataService;
use crate::models::Asset;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingMarketData {
    Price,
    Fx,
}

pub struct ValueInReportingCurrency {
    pub value: Option<String>,
    pub missing: Option<MissingMarketData>,
}

/// Convert an `amount` of `asset` into `reporting_currency` as of `as_of_date`,
/// using cached market data only.
///
/// Returns:
/// - `Ok(Some(v))` when conversion succeeded
/// - `Ok(None)` when required market data is missing (price or FX)
/// - `Err(_)` when inputs are invalid
async fn value_in_reporting_currency_detailed_with_options(
    market_data: &MarketDataService,
    asset: &Asset,
    amount: &str,
    reporting_currency: &str,
    as_of_date: NaiveDate,
    currency_decimals: Option<u32>,
    allow_quote_fallback: bool,
) -> Result<ValueInReportingCurrency> {
    let asset = asset.normalized();
    let amount = Decimal::from_str(amount)
        .with_context(|| format!("Invalid amount for valuation: {amount}"))?;
    let reporting_currency = reporting_currency.trim().to_uppercase();

    match &asset {
        Asset::Currency { iso_code } => {
            if iso_code.eq_ignore_ascii_case(&reporting_currency) {
                return Ok(ValueInReportingCurrency {
                    value: Some(format_base_currency_value(amount, currency_decimals)),
                    missing: None,
                });
            }

            let Some(rate) = market_data
                .fx_from_store(iso_code, &reporting_currency, as_of_date)
                .await?
            else {
                return Ok(ValueInReportingCurrency {
                    value: None,
                    missing: Some(MissingMarketData::Fx),
                });
            };

            let fx_rate = Decimal::from_str(&rate.rate)
                .with_context(|| format!("Invalid FX rate value: {}", rate.rate))?;
            Ok(ValueInReportingCurrency {
                value: Some(format_base_currency_value(
                    amount * fx_rate,
                    currency_decimals,
                )),
                missing: None,
            })
        }
        Asset::Equity { .. } | Asset::Crypto { .. } => {
            let Some(price) = market_data
                .valuation_price_from_store(&asset, as_of_date, allow_quote_fallback)
                .await?
            else {
                return Ok(ValueInReportingCurrency {
                    value: None,
                    missing: Some(MissingMarketData::Price),
                });
            };

            let unit_price = Decimal::from_str(&price.price)
                .with_context(|| format!("Invalid asset price value: {}", price.price))?;
            let value_in_quote = amount * unit_price;

            if price
                .quote_currency
                .eq_ignore_ascii_case(&reporting_currency)
            {
                return Ok(ValueInReportingCurrency {
                    value: Some(format_base_currency_value(
                        value_in_quote,
                        currency_decimals,
                    )),
                    missing: None,
                });
            }

            let Some(rate) = market_data
                .fx_from_store(&price.quote_currency, &reporting_currency, as_of_date)
                .await?
            else {
                return Ok(ValueInReportingCurrency {
                    value: None,
                    missing: Some(MissingMarketData::Fx),
                });
            };

            let fx_rate = Decimal::from_str(&rate.rate)
                .with_context(|| format!("Invalid FX rate value: {}", rate.rate))?;
            Ok(ValueInReportingCurrency {
                value: Some(format_base_currency_value(
                    value_in_quote * fx_rate,
                    currency_decimals,
                )),
                missing: None,
            })
        }
    }
}

pub async fn value_in_reporting_currency_detailed(
    market_data: &MarketDataService,
    asset: &Asset,
    amount: &str,
    reporting_currency: &str,
    as_of_date: NaiveDate,
    currency_decimals: Option<u32>,
) -> Result<ValueInReportingCurrency> {
    value_in_reporting_currency_detailed_with_options(
        market_data,
        asset,
        amount,
        reporting_currency,
        as_of_date,
        currency_decimals,
        false,
    )
    .await
}

pub async fn value_in_reporting_currency_detailed_best_effort(
    market_data: &MarketDataService,
    asset: &Asset,
    amount: &str,
    reporting_currency: &str,
    as_of_date: NaiveDate,
    currency_decimals: Option<u32>,
) -> Result<ValueInReportingCurrency> {
    value_in_reporting_currency_detailed_with_options(
        market_data,
        asset,
        amount,
        reporting_currency,
        as_of_date,
        currency_decimals,
        true,
    )
    .await
}

pub async fn value_in_reporting_currency_best_effort(
    market_data: &MarketDataService,
    asset: &Asset,
    amount: &str,
    reporting_currency: &str,
    as_of_date: NaiveDate,
    currency_decimals: Option<u32>,
) -> Result<Option<String>> {
    Ok(value_in_reporting_currency_detailed_best_effort(
        market_data,
        asset,
        amount,
        reporting_currency,
        as_of_date,
        currency_decimals,
    )
    .await?
    .value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use chrono::{TimeZone, Utc};

    use crate::market_data::{
        AssetId, MarketDataService, MarketDataStore, MemoryMarketDataStore, PriceKind, PricePoint,
    };

    fn quote_point(asset: &Asset, date: NaiveDate, price: &str) -> PricePoint {
        PricePoint {
            asset_id: AssetId::from_asset(asset),
            as_of_date: date,
            timestamp: Utc.with_ymd_and_hms(2026, 2, 19, 20, 0, 0).unwrap(),
            price: price.to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Quote,
            source: "test".to_string(),
        }
    }

    fn close_point(asset: &Asset, date: NaiveDate, price: &str) -> PricePoint {
        PricePoint {
            asset_id: AssetId::from_asset(asset),
            as_of_date: date,
            timestamp: Utc.with_ymd_and_hms(2026, 2, 19, 21, 0, 0).unwrap(),
            price: price.to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }
    }

    #[tokio::test]
    async fn strict_valuation_ignores_quote_but_best_effort_uses_it() -> Result<()> {
        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = MarketDataService::new(store.clone(), None);
        let asset = Asset::equity("FXAIX");
        let date = NaiveDate::from_ymd_opt(2026, 2, 19).unwrap();
        store
            .put_prices(&[quote_point(&asset, date, "239.32")])
            .await?;

        let strict =
            value_in_reporting_currency_detailed(&market_data, &asset, "2", "USD", date, None)
                .await?
                .value;
        assert_eq!(strict, None);

        let best_effort =
            value_in_reporting_currency_best_effort(&market_data, &asset, "2", "USD", date, None)
                .await?;
        assert_eq!(best_effort.as_deref(), Some("478.64"));
        Ok(())
    }

    #[tokio::test]
    async fn best_effort_still_prefers_close_prices() -> Result<()> {
        let store = Arc::new(MemoryMarketDataStore::new());
        let market_data = MarketDataService::new(store.clone(), None);
        let asset = Asset::equity("FXAIX");
        let date = NaiveDate::from_ymd_opt(2026, 2, 19).unwrap();
        store
            .put_prices(&[
                quote_point(&asset, date, "237.99"),
                close_point(&asset, date, "239.32"),
            ])
            .await?;

        let best_effort =
            value_in_reporting_currency_best_effort(&market_data, &asset, "1", "USD", date, None)
                .await?;
        assert_eq!(best_effort.as_deref(), Some("239.32"));
        Ok(())
    }
}

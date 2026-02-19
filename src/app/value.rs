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
pub async fn value_in_reporting_currency_detailed(
    market_data: &MarketDataService,
    asset: &Asset,
    amount: &str,
    reporting_currency: &str,
    as_of_date: NaiveDate,
    currency_decimals: Option<u32>,
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
            let Some(price) = market_data.price_from_store(&asset, as_of_date).await? else {
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


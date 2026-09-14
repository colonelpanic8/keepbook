//! Resolution of the capital gains tax rate and the valuation adjustment
//! arguments the portfolio commands take, plus the virtual account that
//! carries latent tax in a snapshot.

use std::str::FromStr;

use anyhow::{Context, Result};
use rust_decimal::Decimal;

use crate::config::ResolvedConfig;
use crate::format::format_base_currency_value;
use crate::portfolio::{AccountSummary, EquityValuationAdjustment};

pub(super) fn parse_tax_rate_fraction(rate: &str, context: &str) -> Result<Decimal> {
    Decimal::from_str(rate)
        .with_context(|| format!("Invalid {context}: {rate}"))
        .map(|rate| rate / Decimal::from(100))
}

pub(super) fn parse_decimal_arg(value: &str, context: &str) -> Result<Decimal> {
    Decimal::from_str(value).with_context(|| format!("Invalid {context}: {value}"))
}

pub(super) fn resolve_equity_valuation_adjustment(
    equity_change_percent: Option<String>,
    target_pre_tax_total_value: Option<String>,
) -> Result<Option<EquityValuationAdjustment>> {
    match (equity_change_percent, target_pre_tax_total_value) {
        (Some(_), Some(_)) => anyhow::bail!(
            "--equity-change-percent and --target-pre-tax-total-value cannot be used together"
        ),
        (Some(percent), None) => Ok(Some(EquityValuationAdjustment::PercentChange(
            parse_decimal_arg(&percent, "equity change percent")?,
        ))),
        (None, Some(target)) => Ok(Some(EquityValuationAdjustment::TargetPreTaxTotalValue(
            parse_decimal_arg(&target, "target pre-tax total value")?,
        ))),
        (None, None) => Ok(None),
    }
}

pub(super) fn decimal_from_f64(value: f64, context: &str) -> Result<Decimal> {
    Decimal::from_str(&value.to_string()).with_context(|| format!("Invalid {context}: {value}"))
}

pub(super) fn resolve_capital_gains_tax_rate(
    config: &ResolvedConfig,
    cli_percent_rate: Option<String>,
) -> Result<(Option<Decimal>, bool)> {
    let latent_tax = &config.portfolio.latent_capital_gains_tax;
    if let Some(rate) = cli_percent_rate {
        return Ok((
            Some(parse_tax_rate_fraction(&rate, "capital gains tax rate")?),
            latent_tax.enabled,
        ));
    }

    if !latent_tax.enabled {
        return Ok((None, false));
    }

    let rate = latent_tax
        .rate
        .context("portfolio.latent_capital_gains_tax.enabled requires a rate")?;
    Ok((
        Some(decimal_from_f64(
            rate,
            "portfolio.latent_capital_gains_tax.rate",
        )?),
        true,
    ))
}

pub(super) fn apply_latent_tax_virtual_account(
    snapshot: &mut crate::portfolio::PortfolioSnapshot,
    config: &ResolvedConfig,
) -> Result<()> {
    let Some(tax_str) = &snapshot.prospective_capital_gains_tax else {
        return Ok(());
    };
    let tax = Decimal::from_str(tax_str)
        .with_context(|| format!("Invalid prospective_capital_gains_tax: {tax_str}"))?;
    if tax <= Decimal::ZERO {
        return Ok(());
    }

    let total_value = Decimal::from_str(&snapshot.total_value)
        .with_context(|| format!("Invalid total_value: {}", snapshot.total_value))?;
    snapshot.total_value =
        format_base_currency_value(total_value - tax, config.display.currency_decimals);

    if let Some(by_account) = snapshot.by_account.as_mut() {
        by_account.push(AccountSummary {
            account_id: "virtual:latent_capital_gains_tax".to_string(),
            account_name: config
                .portfolio
                .latent_capital_gains_tax
                .account_name
                .clone(),
            connection_name: "Virtual".to_string(),
            value_in_base: Some(format_base_currency_value(
                -tax,
                config.display.currency_decimals,
            )),
        });
    }

    Ok(())
}

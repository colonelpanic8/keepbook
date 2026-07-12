use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use chrono_tz::Tz;
use rust_decimal::Decimal;

use crate::config::{DisplayConfig, ResolvedConfig};
use crate::format::{currency_symbol, format_base_currency_display};
use crate::market_data::{MarketDataServiceBuilder, PriceSourceRegistry};
use crate::models::{Asset, Id, TransactionAnnotation};
use crate::storage::Storage;

use super::classification::{
    effective_transaction_subtags, effective_transaction_tags, provider_virtual_tag_hierarchy,
};
use super::ignore_rules::{TransactionIgnoreInput, TransactionIgnoreMatcher};
use super::value::value_in_reporting_currency_best_effort;
use super::{
    AccountOutput, AllOutput, BalanceOutput, ConnectionOutput, PriceSourceOutput,
    TransactionAnnotationOutput, TransactionOutput,
};

#[derive(Debug, Clone)]
enum TransactionDateTz {
    Utc,
    Local,
    Named(Tz),
}

impl TransactionDateTz {
    fn parse(tz: Option<String>) -> Result<Self> {
        let Some(tz) = tz else {
            return Ok(Self::Utc);
        };
        let trimmed = tz.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("utc") {
            return Ok(Self::Utc);
        }
        if trimmed.eq_ignore_ascii_case("local") || trimmed.eq_ignore_ascii_case("current") {
            return Ok(Self::Local);
        }
        let named = trimmed.parse::<Tz>().with_context(|| {
            format!("Invalid timezone '{trimmed}' (expected IANA name, e.g. America/New_York)")
        })?;
        Ok(Self::Named(named))
    }

    fn date_for(&self, timestamp: chrono::DateTime<Utc>) -> NaiveDate {
        match self {
            Self::Utc => timestamp.date_naive(),
            Self::Local => timestamp.with_timezone(&chrono::Local).date_naive(),
            Self::Named(tz) => timestamp.with_timezone(tz).date_naive(),
        }
    }
}

fn annotation_ignores_spending(annotation: &TransactionAnnotation) -> bool {
    annotation.ignores_spending()
}

pub async fn list_connections(storage: &dyn Storage) -> Result<Vec<ConnectionOutput>> {
    let connections = storage.list_connections().await?;
    let accounts = storage.list_accounts().await?;
    let mut accounts_by_connection: HashMap<Id, HashSet<Id>> = HashMap::new();
    for account in accounts {
        accounts_by_connection
            .entry(account.connection_id.clone())
            .or_default()
            .insert(account.id.clone());
    }
    let mut output = Vec::new();

    for c in connections {
        let valid_ids = accounts_by_connection
            .get(c.id())
            .cloned()
            .unwrap_or_default();
        let mut account_ids: HashSet<Id> = c
            .state
            .account_ids
            .iter()
            .filter(|id| valid_ids.contains(*id))
            .cloned()
            .collect();
        for account_id in valid_ids {
            account_ids.insert(account_id);
        }

        output.push(ConnectionOutput {
            id: c.id().to_string(),
            name: c.config.name.clone(),
            synchronizer: c.config.synchronizer.clone(),
            status: c.state.status.to_string(),
            account_count: account_ids.len(),
            last_sync: c.state.last_sync.as_ref().map(|ls| ls.at.to_rfc3339()),
        });
    }

    Ok(output)
}

pub async fn list_accounts(storage: &dyn Storage) -> Result<Vec<AccountOutput>> {
    let accounts = storage.list_accounts().await?;
    let mut output = Vec::new();

    for a in accounts {
        let exclude_from_portfolio = storage
            .get_account_config(&a.id)?
            .and_then(|config| config.exclude_from_portfolio)
            .unwrap_or(false);

        output.push(AccountOutput {
            id: a.id.to_string(),
            name: a.name.clone(),
            connection_id: a.connection_id.to_string(),
            tags: a.tags.clone(),
            active: a.active,
            exclude_from_portfolio,
        });
    }

    Ok(output)
}

pub fn list_price_sources(data_dir: &Path) -> Result<Vec<PriceSourceOutput>> {
    let mut registry = PriceSourceRegistry::new(data_dir);
    registry.load()?;

    let mut output = Vec::new();
    for s in registry.sources() {
        output.push(PriceSourceOutput {
            name: s.name.clone(),
            source_type: format!("{:?}", s.config.source_type).to_lowercase(),
            enabled: s.config.enabled,
            priority: s.config.priority,
            has_credentials: s.config.credentials.is_some(),
        });
    }

    Ok(output)
}

fn format_currency_amount_display(
    amount: &str,
    currency: &str,
    display: &DisplayConfig,
    symbol_override: Option<&str>,
) -> Option<String> {
    let value = Decimal::from_str(amount).ok()?;
    let currency = currency.trim().to_uppercase();
    let symbol = symbol_override.or_else(|| currency_symbol(&currency));
    let formatted = format_base_currency_display(
        value,
        display.currency_decimals,
        display.currency_grouping,
        symbol,
        display.currency_fixed_decimals,
    );
    Some(if symbol.is_some() {
        formatted
    } else {
        format!("{formatted} {currency}")
    })
}

pub async fn list_balances(
    storage: &dyn Storage,
    config: &ResolvedConfig,
) -> Result<Vec<BalanceOutput>> {
    let market_data = MarketDataServiceBuilder::for_data_dir(&config.data_dir)
        .with_quote_staleness(config.refresh.price_staleness)
        .build()
        .await;

    let connections = storage.list_connections().await?;
    let accounts = storage.list_accounts().await?;
    let mut accounts_by_connection: HashMap<Id, HashSet<Id>> = HashMap::new();
    for account in accounts {
        accounts_by_connection
            .entry(account.connection_id.clone())
            .or_default()
            .insert(account.id);
    }
    let mut output = Vec::new();

    for conn in connections {
        let valid_ids = accounts_by_connection
            .get(conn.id())
            .cloned()
            .unwrap_or_default();
        let mut account_ids = Vec::new();
        let mut seen_ids: HashSet<Id> = HashSet::new();
        for account_id in &conn.state.account_ids {
            if !valid_ids.contains(account_id) {
                continue;
            }
            if seen_ids.insert(account_id.clone()) {
                account_ids.push(account_id.clone());
            }
        }
        for account_id in valid_ids {
            if seen_ids.insert(account_id.clone()) {
                account_ids.push(account_id);
            }
        }

        for account_id in &account_ids {
            if let Some(snapshot) = storage.get_latest_balance_snapshot(account_id).await? {
                for balance in snapshot.balances {
                    let value_in_reporting_currency = value_in_reporting_currency_best_effort(
                        &market_data,
                        &balance.asset,
                        &balance.amount,
                        &config.reporting_currency,
                        snapshot.timestamp.date_naive(),
                        config.display.currency_decimals,
                    )
                    .await?;

                    let reporting_currency = config.reporting_currency.to_uppercase();
                    let reporting_currency_symbol = config
                        .display
                        .currency_symbol
                        .as_deref()
                        .or_else(|| currency_symbol(&reporting_currency));
                    let amount_display = match &balance.asset {
                        Asset::Currency { iso_code } => {
                            let symbol_override =
                                if iso_code.eq_ignore_ascii_case(&reporting_currency) {
                                    reporting_currency_symbol
                                } else {
                                    None
                                };
                            format_currency_amount_display(
                                &balance.amount,
                                iso_code,
                                &config.display,
                                symbol_override,
                            )
                        }
                        Asset::ManualValue { currency, .. } => {
                            let symbol_override =
                                if currency.eq_ignore_ascii_case(&reporting_currency) {
                                    reporting_currency_symbol
                                } else {
                                    None
                                };
                            format_currency_amount_display(
                                &balance.amount,
                                currency,
                                &config.display,
                                symbol_override,
                            )
                        }
                        Asset::Equity { .. } | Asset::Crypto { .. } => None,
                    };
                    let value_in_reporting_currency_display =
                        value_in_reporting_currency.as_deref().and_then(|value| {
                            format_currency_amount_display(
                                value,
                                &reporting_currency,
                                &config.display,
                                reporting_currency_symbol,
                            )
                        });

                    output.push(BalanceOutput {
                        account_id: account_id.to_string(),
                        asset: serde_json::to_value(&balance.asset)?,
                        amount: balance.amount,
                        amount_display,
                        cost_basis: balance.cost_basis,
                        value_in_reporting_currency,
                        value_in_reporting_currency_display,
                        reporting_currency,
                        reporting_currency_symbol: reporting_currency_symbol.map(str::to_string),
                        timestamp: snapshot.timestamp.to_rfc3339(),
                    });
                }
            }
        }
    }

    Ok(output)
}

pub async fn list_transactions(
    storage: &dyn Storage,
    start: Option<String>,
    end: Option<String>,
    tz: Option<String>,
    sort_by_amount: bool,
    skip_ignored: bool,
    config: &ResolvedConfig,
) -> Result<Vec<TransactionOutput>> {
    let date_tz = TransactionDateTz::parse(tz)?;
    let end_date = match &end {
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .with_context(|| format!("Invalid end date: {s}"))?,
        None => date_tz.date_for(Utc::now()),
    };
    let start_date = match &start {
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .with_context(|| format!("Invalid start date: {s}"))?,
        None => end_date - chrono::Duration::days(30),
    };

    let ignore_matcher = if skip_ignored {
        Some(TransactionIgnoreMatcher::from_configs(
            &config.ignore,
            &config.spending,
        )?)
    } else {
        None
    };
    let accounts = storage.list_accounts().await?;
    let connections = storage.list_connections().await?;
    let connections_by_id: HashMap<String, crate::models::Connection> = connections
        .into_iter()
        .map(|c| (c.id().to_string(), c))
        .collect();
    let mut output = Vec::new();
    let ignored_account_tags: HashSet<String> = if skip_ignored {
        config
            .spending
            .ignore_tags
            .iter()
            .filter_map(|tag| {
                let trimmed = tag.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_lowercase())
                }
            })
            .collect()
    } else {
        HashSet::new()
    };

    for account in accounts {
        if skip_ignored
            && !ignored_account_tags.is_empty()
            && account.tags.iter().any(|tag| {
                let trimmed = tag.trim();
                !trimmed.is_empty() && ignored_account_tags.contains(&trimmed.to_lowercase())
            })
        {
            continue;
        }

        let connection = connections_by_id.get(&account.connection_id.to_string());
        let connection_id = account.connection_id.to_string();
        let connection_name = connection
            .map(|c| c.config.name.as_str())
            .unwrap_or_default();
        let synchronizer = connection
            .map(|c| c.config.synchronizer.as_str())
            .unwrap_or_default();

        let transactions = storage.get_transactions(&account.id).await?;
        let patches = storage
            .get_transaction_annotation_patches(&account.id)
            .await?;

        // Materialize last-write-wins annotation state per transaction id.
        let mut annotations_by_tx: HashMap<Id, TransactionAnnotation> = HashMap::new();
        for patch in patches {
            let tx_id = patch.transaction_id.clone();
            let ann = annotations_by_tx
                .entry(tx_id.clone())
                .or_insert_with(|| TransactionAnnotation::new(tx_id));
            patch.apply_to(ann);
        }

        for tx in transactions {
            let ann = annotations_by_tx.get(&tx.id);
            let tx_date = ann
                .and_then(|annotation| annotation.effective_date)
                .unwrap_or_else(|| date_tz.date_for(tx.timestamp));
            if tx_date < start_date || tx_date > end_date {
                continue;
            }
            let status = format!("{:?}", tx.status).to_lowercase();

            if skip_ignored {
                if tx
                    .standardized_metadata
                    .as_ref()
                    .and_then(|md| md.is_internal_transfer_hint)
                    .unwrap_or(false)
                {
                    continue;
                }
                if ignore_matcher.as_ref().is_some_and(|matcher| {
                    matcher.is_match(&TransactionIgnoreInput {
                        account_id: account.id.as_str(),
                        account_name: &account.name,
                        connection_id: &connection_id,
                        connection_name,
                        synchronizer,
                        description: &tx.description,
                        status: &status,
                        amount: &tx.amount,
                    })
                }) {
                    continue;
                }
            }

            let annotation = annotations_by_tx.get(&tx.id).and_then(|ann| {
                if ann.is_empty() {
                    None
                } else {
                    Some(TransactionAnnotationOutput {
                        description: ann.description.clone(),
                        note: ann.note.clone(),
                        tags: ann.tags.clone(),
                        subtags: ann.subtags.clone(),
                        effective_date: ann.effective_date.map(|d| d.to_string()),
                        ignore_spending: ann.ignore_spending,
                    })
                }
            });
            let raw_annotation = annotations_by_tx.get(&tx.id);
            let provider_hierarchy = provider_virtual_tag_hierarchy(
                tx.standardized_metadata.as_ref(),
                &tx.synchronizer_data,
                &config.tags,
            );
            let tags =
                effective_transaction_tags(raw_annotation, &provider_hierarchy, &config.tags);
            let subtags =
                effective_transaction_subtags(raw_annotation, &provider_hierarchy, &config.tags);
            if skip_ignored
                && annotations_by_tx
                    .get(&tx.id)
                    .is_some_and(annotation_ignores_spending)
            {
                continue;
            }

            output.push(TransactionOutput {
                id: tx.id.to_string(),
                account_id: account.id.to_string(),
                account_name: account.name.clone(),
                timestamp: tx.timestamp.to_rfc3339(),
                description: tx.description.clone(),
                amount: tx.amount.clone(),
                asset: serde_json::to_value(&tx.asset).unwrap_or_default(),
                status,
                tags,
                subtags,
                annotation,
                standardized_metadata: tx.standardized_metadata.clone(),
            });
        }
    }

    if sort_by_amount {
        output.sort_by(|a, b| {
            let left = Decimal::from_str(&a.amount);
            let right = Decimal::from_str(&b.amount);
            match (left, right) {
                (Ok(la), Ok(rb)) => la.cmp(&rb),
                (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
                (Ok(_), Err(_)) => std::cmp::Ordering::Less,
                (Err(_), Err(_)) => a.amount.cmp(&b.amount),
            }
        });
    }

    Ok(output)
}

pub async fn list_all(storage: &dyn Storage, config: &ResolvedConfig) -> Result<AllOutput> {
    Ok(AllOutput {
        connections: list_connections(storage).await?,
        accounts: list_accounts(storage).await?,
        price_sources: list_price_sources(&config.data_dir)?,
        balances: list_balances(storage, config).await?,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/app/list_tests.rs"]
mod list_tests;

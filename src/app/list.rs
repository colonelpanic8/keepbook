use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::NaiveDate;

use crate::config::ResolvedConfig;
use crate::market_data::{MarketDataService, MarketDataServiceBuilder, PriceSourceRegistry};
use crate::models::{Asset, Id, TransactionAnnotation};
use crate::storage::Storage;

use super::{
    AccountOutput, AllOutput, BalanceOutput, ConnectionOutput, PriceSourceOutput,
    TransactionAnnotationOutput, TransactionOutput,
};
use crate::format::format_base_currency_value;

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
        output.push(AccountOutput {
            id: a.id.to_string(),
            name: a.name.clone(),
            connection_id: a.connection_id.to_string(),
            tags: a.tags.clone(),
            active: a.active,
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

async fn value_in_reporting_currency(
    market_data: &MarketDataService,
    asset: &Asset,
    amount: &str,
    reporting_currency: &str,
    as_of_date: NaiveDate,
    currency_decimals: Option<u32>,
) -> Result<Option<String>> {
    use rust_decimal::Decimal;

    let amount = Decimal::from_str(amount)
        .with_context(|| format!("Invalid balance amount for valuation: {amount}"))?;
    let reporting_currency = reporting_currency.trim().to_uppercase();

    match asset {
        Asset::Currency { iso_code } => {
            if iso_code.eq_ignore_ascii_case(&reporting_currency) {
                return Ok(Some(format_base_currency_value(amount, currency_decimals)));
            }

            let Some(rate) = market_data
                .fx_from_store(iso_code, &reporting_currency, as_of_date)
                .await?
            else {
                return Ok(None);
            };

            let fx_rate = Decimal::from_str(&rate.rate)
                .with_context(|| format!("Invalid FX rate value: {}", rate.rate))?;
            Ok(Some(format_base_currency_value(
                amount * fx_rate,
                currency_decimals,
            )))
        }
        Asset::Equity { .. } | Asset::Crypto { .. } => {
            let Some(price) = market_data.price_from_store(asset, as_of_date).await? else {
                return Ok(None);
            };

            let unit_price = Decimal::from_str(&price.price)
                .with_context(|| format!("Invalid asset price value: {}", price.price))?;
            let value_in_quote = amount * unit_price;

            if price
                .quote_currency
                .eq_ignore_ascii_case(&reporting_currency)
            {
                return Ok(Some(format_base_currency_value(
                    value_in_quote,
                    currency_decimals,
                )));
            }

            let Some(rate) = market_data
                .fx_from_store(&price.quote_currency, &reporting_currency, as_of_date)
                .await?
            else {
                return Ok(None);
            };

            let fx_rate = Decimal::from_str(&rate.rate)
                .with_context(|| format!("Invalid FX rate value: {}", rate.rate))?;
            Ok(Some(format_base_currency_value(
                value_in_quote * fx_rate,
                currency_decimals,
            )))
        }
    }
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
                    let value_in_reporting_currency = value_in_reporting_currency(
                        &market_data,
                        &balance.asset,
                        &balance.amount,
                        &config.reporting_currency,
                        snapshot.timestamp.date_naive(),
                        config.display.currency_decimals,
                    )
                    .await?;

                    output.push(BalanceOutput {
                        account_id: account_id.to_string(),
                        asset: serde_json::to_value(&balance.asset)?,
                        amount: balance.amount,
                        value_in_reporting_currency,
                        reporting_currency: config.reporting_currency.to_uppercase(),
                        timestamp: snapshot.timestamp.to_rfc3339(),
                    });
                }
            }
        }
    }

    Ok(output)
}

pub async fn list_transactions(storage: &dyn Storage) -> Result<Vec<TransactionOutput>> {
    let accounts = storage.list_accounts().await?;
    let mut output = Vec::new();

    for account in accounts {
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
            let annotation = annotations_by_tx.get(&tx.id).and_then(|ann| {
                if ann.is_empty() {
                    None
                } else {
                    Some(TransactionAnnotationOutput {
                        description: ann.description.clone(),
                        note: ann.note.clone(),
                        category: ann.category.clone(),
                        tags: ann.tags.clone(),
                    })
                }
            });

            output.push(TransactionOutput {
                id: tx.id.to_string(),
                account_id: account.id.to_string(),
                timestamp: tx.timestamp.to_rfc3339(),
                description: tx.description.clone(),
                amount: tx.amount.clone(),
                asset: serde_json::to_value(&tx.asset).unwrap_or_default(),
                status: format!("{:?}", tx.status).to_lowercase(),
                annotation,
            });
        }
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
mod tests {
    use super::*;
    use crate::clock::{Clock, FixedClock};
    use crate::models::{
        Account, Asset, FixedIdGenerator, Transaction, TransactionAnnotationPatch,
    };
    use crate::storage::MemoryStorage;
    use chrono::{TimeZone, Utc};

    #[tokio::test]
    async fn list_transactions_includes_annotation_when_present() -> Result<()> {
        let storage = MemoryStorage::new();
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

        let account_id = Id::from_string("acct-1");
        let account = Account::new_with(
            account_id.clone(),
            clock.now(),
            "Checking",
            Id::from_string("conn-1"),
        );
        storage.save_account(&account).await?;

        let ids = FixedIdGenerator::new([Id::from_string("tx-1")]);
        let tx = Transaction::new_with_generator(&ids, &clock, "-1", Asset::currency("USD"), "RAW");
        storage.append_transactions(&account_id, &[tx]).await?;

        let patch = TransactionAnnotationPatch {
            transaction_id: Id::from_string("tx-1"),
            timestamp: clock.now(),
            description: None,
            note: None,
            category: Some(Some("food".to_string())),
            tags: Some(Some(vec!["coffee".to_string()])),
        };
        storage
            .append_transaction_annotation_patches(&account_id, &[patch])
            .await?;

        let out = list_transactions(&storage).await?;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "tx-1");
        assert_eq!(
            out[0].annotation.as_ref().unwrap().category.as_deref(),
            Some("food")
        );
        assert_eq!(
            out[0].annotation.as_ref().unwrap().tags.clone().unwrap(),
            vec!["coffee".to_string()]
        );
        Ok(())
    }
}

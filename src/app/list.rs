use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use anyhow::Result;
use rust_decimal::Decimal;

use crate::config::ResolvedConfig;
use crate::market_data::{MarketDataServiceBuilder, PriceSourceRegistry};
use crate::models::{Id, TransactionAnnotation};
use crate::storage::Storage;

use super::value::value_in_reporting_currency_best_effort;
use super::{
    AccountOutput, AllOutput, BalanceOutput, ConnectionOutput, PriceSourceOutput,
    TransactionAnnotationOutput, TransactionOutput,
};

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

fn normalized_rule(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_lowercase())
    }
}

async fn ignored_account_ids_for_portfolio_spending(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    accounts: &[crate::models::Account],
) -> Result<HashSet<Id>> {
    let ignore_accounts: HashSet<String> = config
        .spending
        .ignore_accounts
        .iter()
        .filter_map(|s| normalized_rule(s))
        .collect();
    let ignore_connections_raw: HashSet<String> = config
        .spending
        .ignore_connections
        .iter()
        .filter_map(|s| normalized_rule(s))
        .collect();
    let ignore_tags: HashSet<String> = config
        .spending
        .ignore_tags
        .iter()
        .filter_map(|s| normalized_rule(s))
        .collect();

    if ignore_accounts.is_empty() && ignore_connections_raw.is_empty() && ignore_tags.is_empty() {
        return Ok(HashSet::new());
    }

    let connections = storage.list_connections().await?;
    let mut ignore_connections: HashSet<String> = ignore_connections_raw.clone();
    for conn in connections {
        let conn_id = conn.id().to_string().to_lowercase();
        let conn_name = conn.config.name.to_lowercase();
        if ignore_connections_raw.contains(&conn_id) || ignore_connections_raw.contains(&conn_name)
        {
            ignore_connections.insert(conn_id);
        }
    }

    let mut ignored = HashSet::new();
    for account in accounts {
        let account_id = account.id.to_string().to_lowercase();
        let account_name = account.name.to_lowercase();
        let connection_id = account.connection_id.to_string().to_lowercase();
        let has_ignored_tag = account
            .tags
            .iter()
            .filter_map(|tag| normalized_rule(tag))
            .any(|tag| ignore_tags.contains(&tag));

        if ignore_accounts.contains(&account_id)
            || ignore_accounts.contains(&account_name)
            || ignore_connections.contains(&connection_id)
            || has_ignored_tag
        {
            ignored.insert(account.id.clone());
        }
    }

    Ok(ignored)
}

pub async fn list_transactions(
    storage: &dyn Storage,
    sort_by_amount: bool,
    skip_spending_ignored: bool,
    config: &ResolvedConfig,
) -> Result<Vec<TransactionOutput>> {
    let accounts = storage.list_accounts().await?;
    let ignored_account_ids = if skip_spending_ignored {
        ignored_account_ids_for_portfolio_spending(storage, config, &accounts).await?
    } else {
        HashSet::new()
    };
    let mut output = Vec::new();

    for account in accounts {
        if skip_spending_ignored && ignored_account_ids.contains(&account.id) {
            continue;
        }

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
mod tests {
    use super::*;
    use crate::clock::{Clock, FixedClock};
    use crate::models::{
        Account, Asset, FixedIdGenerator, Transaction, TransactionAnnotationPatch,
    };
    use crate::storage::MemoryStorage;
    use chrono::{TimeZone, Utc};

    fn test_config() -> ResolvedConfig {
        ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig::default(),
            git: crate::config::GitConfig::default(),
        }
    }

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

        let out = list_transactions(&storage, false, true, &test_config()).await?;
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

    #[tokio::test]
    async fn list_transactions_sorts_by_amount_when_requested() -> Result<()> {
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

        let ids = FixedIdGenerator::new([
            Id::from_string("tx-1"),
            Id::from_string("tx-2"),
            Id::from_string("tx-3"),
        ]);
        let tx1 = Transaction::new_with_generator(&ids, &clock, "10", Asset::currency("USD"), "A");
        let tx2 = Transaction::new_with_generator(&ids, &clock, "-2.50", Asset::currency("USD"), "B");
        let tx3 = Transaction::new_with_generator(&ids, &clock, "1.25", Asset::currency("USD"), "C");
        storage
            .append_transactions(&account_id, &[tx1, tx2, tx3])
            .await?;

        let out = list_transactions(&storage, true, true, &test_config()).await?;
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].id, "tx-2");
        assert_eq!(out[1].id, "tx-3");
        assert_eq!(out[2].id, "tx-1");
        Ok(())
    }

    #[tokio::test]
    async fn list_transactions_skips_spending_ignored_accounts_by_default() -> Result<()> {
        let storage = MemoryStorage::new();
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

        let account_id = Id::from_string("acct-1");
        let account = Account::new_with(
            account_id.clone(),
            clock.now(),
            "Ignore Me",
            Id::from_string("conn-1"),
        );
        storage.save_account(&account).await?;

        let ids = FixedIdGenerator::new([Id::from_string("tx-1")]);
        let tx = Transaction::new_with_generator(&ids, &clock, "1", Asset::currency("USD"), "Test");
        storage.append_transactions(&account_id, &[tx]).await?;

        let mut config = test_config();
        config.spending.ignore_accounts = vec!["Ignore Me".to_string()];

        let skipped = list_transactions(&storage, false, true, &config).await?;
        assert_eq!(skipped.len(), 0);

        let included = list_transactions(&storage, false, false, &config).await?;
        assert_eq!(included.len(), 1);
        Ok(())
    }
}

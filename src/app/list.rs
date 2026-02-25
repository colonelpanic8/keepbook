use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use rust_decimal::Decimal;

use crate::config::ResolvedConfig;
use crate::market_data::{MarketDataServiceBuilder, PriceSourceRegistry};
use crate::models::{Id, TransactionAnnotation};
use crate::storage::Storage;

use super::ignore_rules::{TransactionIgnoreInput, TransactionIgnoreMatcher};
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

pub async fn list_transactions(
    storage: &dyn Storage,
    start: Option<String>,
    end: Option<String>,
    sort_by_amount: bool,
    skip_ignored: bool,
    config: &ResolvedConfig,
) -> Result<Vec<TransactionOutput>> {
    let end_date = match &end {
        Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .with_context(|| format!("Invalid end date: {s}"))?,
        None => Utc::now().date_naive(),
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
            let tx_date = tx.timestamp.date_naive();
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
                        category: ann.category.clone(),
                        tags: ann.tags.clone(),
                    })
                }
            });

            output.push(TransactionOutput {
                id: tx.id.to_string(),
                account_id: account.id.to_string(),
                account_name: account.name.clone(),
                timestamp: tx.timestamp.to_rfc3339(),
                description: tx.description.clone(),
                amount: tx.amount.clone(),
                asset: serde_json::to_value(&tx.asset).unwrap_or_default(),
                status,
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
mod tests {
    use super::*;
    use crate::clock::{Clock, FixedClock};
    use crate::models::{
        Account, Asset, FixedIdGenerator, Transaction, TransactionAnnotationPatch,
        TransactionStandardizedMetadata,
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

        let out = list_transactions(
            &storage,
            None,
            None,
            false,
            true,
            &ResolvedConfig {
                data_dir: std::path::PathBuf::from("/tmp"),
                reporting_currency: "USD".to_string(),
                display: crate::config::DisplayConfig::default(),
                refresh: crate::config::RefreshConfig::default(),
                tray: crate::config::TrayConfig::default(),
                spending: crate::config::SpendingConfig::default(),
                ignore: crate::config::IgnoreConfig::default(),
                git: crate::config::GitConfig::default(),
            },
        )
        .await?;
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
        let tx2 =
            Transaction::new_with_generator(&ids, &clock, "-2.50", Asset::currency("USD"), "B");
        let tx3 =
            Transaction::new_with_generator(&ids, &clock, "1.25", Asset::currency("USD"), "C");
        storage
            .append_transactions(&account_id, &[tx1, tx2, tx3])
            .await?;

        let out = list_transactions(
            &storage,
            Some("2000-01-01".to_string()),
            Some("2099-12-31".to_string()),
            true,
            true,
            &ResolvedConfig {
                data_dir: std::path::PathBuf::from("/tmp"),
                reporting_currency: "USD".to_string(),
                display: crate::config::DisplayConfig::default(),
                refresh: crate::config::RefreshConfig::default(),
                tray: crate::config::TrayConfig::default(),
                spending: crate::config::SpendingConfig::default(),
                ignore: crate::config::IgnoreConfig::default(),
                git: crate::config::GitConfig::default(),
            },
        )
        .await?;

        assert_eq!(out.len(), 3);
        assert_eq!(out[0].id, "tx-2");
        assert_eq!(out[1].id, "tx-3");
        assert_eq!(out[2].id, "tx-1");
        Ok(())
    }

    #[tokio::test]
    async fn list_transactions_can_include_ignored_when_requested() -> Result<()> {
        let storage = MemoryStorage::new();
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

        let account_id = Id::from_string("acct-1");
        let account = Account::new_with(
            account_id.clone(),
            clock.now(),
            "Investor Checking",
            Id::from_string("conn-1"),
        );
        storage.save_account(&account).await?;

        let ids = FixedIdGenerator::new([Id::from_string("tx-1"), Id::from_string("tx-2")]);
        let tx1 = Transaction::new_with_generator(
            &ids,
            &clock,
            "-500",
            Asset::currency("USD"),
            "ACH CHASE CREDIT CRD EPAY",
        );
        let tx2 = Transaction::new_with_generator(
            &ids,
            &clock,
            "-2500",
            Asset::currency("USD"),
            "BALLAST WEB PMTS",
        );
        storage
            .append_transactions(&account_id, &[tx1, tx2])
            .await?;

        let config = ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig::default(),
            ignore: crate::config::IgnoreConfig {
                transaction_rules: vec![crate::config::TransactionIgnoreRule {
                    account_id: None,
                    account_name: Some("(?i)^Investor Checking$".to_string()),
                    connection_id: None,
                    connection_name: None,
                    synchronizer: None,
                    description: Some("(?i)credit\\s+crd\\s+(?:e?pay|autopay)".to_string()),
                    status: None,
                    amount: None,
                }],
            },
            git: crate::config::GitConfig::default(),
        };

        let skipped = list_transactions(
            &storage,
            Some("2000-01-01".to_string()),
            Some("2099-12-31".to_string()),
            false,
            true,
            &config,
        )
        .await?;
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].id, "tx-2");

        let included = list_transactions(
            &storage,
            Some("2000-01-01".to_string()),
            Some("2099-12-31".to_string()),
            false,
            false,
            &config,
        )
        .await?;
        assert_eq!(included.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn list_transactions_applies_spending_account_ignore_rules() -> Result<()> {
        let storage = MemoryStorage::new();
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

        let account_id = Id::from_string("acct-1");
        let account = Account::new_with(
            account_id.clone(),
            clock.now(),
            "Investor Checking",
            Id::from_string("conn-1"),
        );
        storage.save_account(&account).await?;

        let ids = FixedIdGenerator::new([Id::from_string("tx-1")]);
        let tx = Transaction::new_with_generator(
            &ids,
            &clock,
            "-500",
            Asset::currency("USD"),
            "ACH CHASE CREDIT CRD EPAY",
        );
        storage.append_transactions(&account_id, &[tx]).await?;

        let config = ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig {
                ignore_accounts: vec!["Investor Checking".to_string()],
                ignore_connections: vec![],
                ignore_tags: vec![],
            },
            ignore: crate::config::IgnoreConfig::default(),
            git: crate::config::GitConfig::default(),
        };

        let skipped = list_transactions(
            &storage,
            Some("2000-01-01".to_string()),
            Some("2099-12-31".to_string()),
            false,
            true,
            &config,
        )
        .await?;
        assert!(skipped.is_empty());

        let included = list_transactions(
            &storage,
            Some("2000-01-01".to_string()),
            Some("2099-12-31".to_string()),
            false,
            false,
            &config,
        )
        .await?;
        assert_eq!(included.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn list_transactions_applies_spending_ignore_tags() -> Result<()> {
        let storage = MemoryStorage::new();
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

        let account_id = Id::from_string("acct-1");
        let mut account = Account::new_with(
            account_id.clone(),
            clock.now(),
            "Individual",
            Id::from_string("conn-1"),
        );
        account.tags = vec!["brokerage".to_string()];
        storage.save_account(&account).await?;

        let ids = FixedIdGenerator::new([Id::from_string("tx-1")]);
        let tx = Transaction::new_with_generator(
            &ids,
            &clock,
            "-19883.99",
            Asset::currency("USD"),
            "Buy ADBE ADOBE INC",
        );
        storage.append_transactions(&account_id, &[tx]).await?;

        let config = ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig {
                ignore_accounts: vec![],
                ignore_connections: vec![],
                ignore_tags: vec!["brokerage".to_string()],
            },
            ignore: crate::config::IgnoreConfig::default(),
            git: crate::config::GitConfig::default(),
        };

        let skipped = list_transactions(
            &storage,
            Some("2000-01-01".to_string()),
            Some("2099-12-31".to_string()),
            false,
            true,
            &config,
        )
        .await?;
        assert!(skipped.is_empty());

        let included = list_transactions(
            &storage,
            Some("2000-01-01".to_string()),
            Some("2099-12-31".to_string()),
            false,
            false,
            &config,
        )
        .await?;
        assert_eq!(included.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn list_transactions_ignores_internal_transfer_hints_when_skipping_ignored() -> Result<()>
    {
        let storage = MemoryStorage::new();
        let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 18, 12, 0, 0).unwrap());

        let account_id = Id::from_string("acct-1");
        let account = Account::new_with(
            account_id.clone(),
            clock.now(),
            "Sapphire Reserve (6395)",
            Id::from_string("conn-1"),
        );
        storage.save_account(&account).await?;

        let ids = FixedIdGenerator::new([Id::from_string("tx-1")]);
        let mut tx = Transaction::new_with_generator(
            &ids,
            &clock,
            "-4450.62",
            Asset::currency("USD"),
            "Payment Thank You - Web",
        );
        tx.standardized_metadata = Some(TransactionStandardizedMetadata {
            transaction_kind: Some("payment".to_string()),
            is_internal_transfer_hint: Some(true),
            ..TransactionStandardizedMetadata::default()
        });
        storage.append_transactions(&account_id, &[tx]).await?;

        let config = ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            git: crate::config::GitConfig::default(),
        };

        let skipped = list_transactions(
            &storage,
            Some("2000-01-01".to_string()),
            Some("2099-12-31".to_string()),
            false,
            true,
            &config,
        )
        .await?;
        assert!(skipped.is_empty());

        let included = list_transactions(
            &storage,
            Some("2000-01-01".to_string()),
            Some("2099-12-31".to_string()),
            false,
            false,
            &config,
        )
        .await?;
        assert_eq!(included.len(), 1);
        Ok(())
    }
}

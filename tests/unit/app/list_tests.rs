use super::*;
use crate::clock::{Clock, FixedClock};
use crate::models::{
    Account, AccountConfig, Asset, FixedIdGenerator, Transaction, TransactionAnnotationPatch,
    TransactionStandardizedMetadata,
};
use crate::storage::MemoryStorage;
use chrono::{TimeZone, Utc};

#[tokio::test]
async fn list_accounts_marks_portfolio_excluded_accounts() -> Result<()> {
    let storage = MemoryStorage::new();
    let account_id = Id::from_string("acct-1");
    let account = Account::new_with(
        account_id.clone(),
        Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap(),
        "Mortgage",
        Id::from_string("conn-1"),
    );
    storage.save_account(&account).await?;
    storage
        .save_account_config(
            &account_id,
            &AccountConfig {
                exclude_from_portfolio: Some(true),
                ..AccountConfig::default()
            },
        )
        .await?;

    let accounts = list_accounts(&storage).await?;

    assert_eq!(accounts.len(), 1);
    assert!(accounts[0].exclude_from_portfolio);
    Ok(())
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
        tags: Some(Some(vec!["food".to_string()])),
        subtags: Some(Some(vec!["coffee".to_string()])),
        effective_date: None,
        ignore_spending: None,
    };
    storage
        .append_transaction_annotation_patches(&account_id, &[patch])
        .await?;

    let out = list_transactions(
        &storage,
        Some("2000-01-01".to_string()),
        Some("2099-12-31".to_string()),
        None,
        false,
        true,
        &ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            history: crate::config::HistoryConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig::default(),
            tags: Default::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            ai: crate::config::AiConfig::default(),
            git: crate::config::GitConfig::default(),
        },
    )
    .await?;
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id, "tx-1");
    assert_eq!(
        out[0].annotation.as_ref().unwrap().subtags.clone().unwrap(),
        vec!["coffee".to_string()]
    );
    assert_eq!(
        out[0].annotation.as_ref().unwrap().tags.clone().unwrap(),
        vec!["food".to_string()]
    );
    assert_eq!(out[0].tags, vec!["food".to_string()]);
    assert_eq!(out[0].subtags, vec!["coffee".to_string()]);
    Ok(())
}

#[tokio::test]
async fn list_transactions_uses_configured_tag_hierarchy_for_provider_labels() -> Result<()> {
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
    let tx = Transaction::new_with_generator(
        &ids,
        &clock,
        "-12",
        Asset::currency("USD"),
        "Grocery Store",
    )
    .with_timestamp(clock.now())
    .with_synchronizer_data(serde_json::json!({
        "category": ["Food and Drink", "Groceries"]
    }));
    storage.append_transactions(&account_id, &[tx]).await?;

    let out = list_transactions(
        &storage,
        Some("2000-01-01".to_string()),
        Some("2099-12-31".to_string()),
        None,
        false,
        true,
        &ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            history: crate::config::HistoryConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig::default(),
            tags: crate::config::TagsConfig {
                aliases: HashMap::from([("Food And Drink".to_string(), "Food".to_string())]),
                parents: HashMap::from([("Groceries".to_string(), vec!["Food".to_string()])]),
            },
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            ai: crate::config::AiConfig::default(),
            git: crate::config::GitConfig::default(),
        },
    )
    .await?;

    assert_eq!(out.len(), 1);
    assert_eq!(out[0].tags, vec!["Food".to_string()]);
    assert_eq!(out[0].subtags, vec!["Groceries".to_string()]);
    Ok(())
}

#[tokio::test]
async fn list_transactions_filters_by_annotation_effective_date() -> Result<()> {
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
    let tx = Transaction::new_with_generator(&ids, &clock, "-1", Asset::currency("USD"), "Rent");
    storage.append_transactions(&account_id, &[tx]).await?;

    storage
        .append_transaction_annotation_patches(
            &account_id,
            &[TransactionAnnotationPatch {
                transaction_id: Id::from_string("tx-1"),
                timestamp: clock.now(),
                description: None,
                note: None,
                tags: None,
                subtags: None,
                effective_date: Some(Some(chrono::NaiveDate::from_ymd_opt(2026, 1, 31).unwrap())),
                ignore_spending: None,
            }],
        )
        .await?;

    let out = list_transactions(
        &storage,
        Some("2026-01-31".to_string()),
        Some("2026-01-31".to_string()),
        None,
        false,
        true,
        &ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            history: crate::config::HistoryConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig::default(),
            tags: Default::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            ai: crate::config::AiConfig::default(),
            git: crate::config::GitConfig::default(),
        },
    )
    .await?;

    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0]
            .annotation
            .as_ref()
            .unwrap()
            .effective_date
            .as_deref(),
        Some("2026-01-31")
    );
    assert_eq!(out[0].timestamp, "2026-02-05T12:00:00+00:00");
    Ok(())
}

#[tokio::test]
async fn list_transactions_can_filter_by_named_timezone_date() -> Result<()> {
    let storage = MemoryStorage::new();
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 1, 7, 30, 0).unwrap());

    let account_id = Id::from_string("acct-1");
    let account = Account::new_with(
        account_id.clone(),
        clock.now(),
        "Checking",
        Id::from_string("conn-1"),
    );
    storage.save_account(&account).await?;

    let ids = FixedIdGenerator::new([Id::from_string("tx-1")]);
    let tx = Transaction::new_with_generator(&ids, &clock, "-1", Asset::currency("USD"), "Coffee");
    storage.append_transactions(&account_id, &[tx]).await?;

    let config = ResolvedConfig {
        data_dir: std::path::PathBuf::from("/tmp"),
        reporting_currency: "USD".to_string(),
        display: crate::config::DisplayConfig::default(),
        refresh: crate::config::RefreshConfig::default(),
        history: crate::config::HistoryConfig::default(),
        tray: crate::config::TrayConfig::default(),
        spending: crate::config::SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: crate::config::GitConfig::default(),
    };

    let utc_rows = list_transactions(
        &storage,
        Some("2026-01-31".to_string()),
        Some("2026-01-31".to_string()),
        None,
        false,
        true,
        &config,
    )
    .await?;
    assert!(utc_rows.is_empty());

    let pacific_rows = list_transactions(
        &storage,
        Some("2026-01-31".to_string()),
        Some("2026-01-31".to_string()),
        Some("America/Los_Angeles".to_string()),
        false,
        true,
        &config,
    )
    .await?;
    assert_eq!(pacific_rows.len(), 1);
    assert_eq!(pacific_rows[0].id, "tx-1");
    Ok(())
}

#[tokio::test]
async fn list_transactions_skips_annotation_ignore_spending_tags() -> Result<()> {
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
    let tx = Transaction::new_with_generator(
        &ids,
        &clock,
        "-30000",
        Asset::currency("USD"),
        "WIRE Outgoing Wire",
    );
    storage.append_transactions(&account_id, &[tx]).await?;

    storage
        .append_transaction_annotation_patches(
            &account_id,
            &[TransactionAnnotationPatch {
                transaction_id: Id::from_string("tx-1"),
                timestamp: clock.now(),
                description: None,
                note: Some(Some("Transfer; ignored from spending".to_string())),
                tags: Some(Some(vec!["ignore_spending".to_string()])),
                subtags: None,
                effective_date: None,
                ignore_spending: None,
            }],
        )
        .await?;

    let skipped = list_transactions(
        &storage,
        Some("2000-01-01".to_string()),
        Some("2099-12-31".to_string()),
        None,
        false,
        true,
        &ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            history: crate::config::HistoryConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig::default(),
            tags: Default::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            ai: crate::config::AiConfig::default(),
            git: crate::config::GitConfig::default(),
        },
    )
    .await?;
    assert!(skipped.is_empty());

    let included = list_transactions(
        &storage,
        Some("2000-01-01".to_string()),
        Some("2099-12-31".to_string()),
        None,
        false,
        false,
        &ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            history: crate::config::HistoryConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig::default(),
            tags: Default::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            ai: crate::config::AiConfig::default(),
            git: crate::config::GitConfig::default(),
        },
    )
    .await?;
    assert_eq!(included.len(), 1);
    assert!(included[0].annotation.is_some());
    Ok(())
}

#[tokio::test]
async fn list_transactions_skips_annotation_ignore_spending_flag() -> Result<()> {
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
    let tx = Transaction::new_with_generator(
        &ids,
        &clock,
        "-30000",
        Asset::currency("USD"),
        "WIRE Outgoing Wire",
    );
    storage.append_transactions(&account_id, &[tx]).await?;

    storage
        .append_transaction_annotation_patches(
            &account_id,
            &[TransactionAnnotationPatch {
                transaction_id: Id::from_string("tx-1"),
                timestamp: clock.now(),
                description: None,
                note: None,
                tags: None,
                subtags: None,
                effective_date: None,
                ignore_spending: Some(Some(true)),
            }],
        )
        .await?;

    let config = ResolvedConfig {
        data_dir: std::path::PathBuf::from("/tmp"),
        reporting_currency: "USD".to_string(),
        display: crate::config::DisplayConfig::default(),
        refresh: crate::config::RefreshConfig::default(),
        history: crate::config::HistoryConfig::default(),
        tray: crate::config::TrayConfig::default(),
        spending: crate::config::SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: crate::config::GitConfig::default(),
    };

    let skipped = list_transactions(
        &storage,
        Some("2000-01-01".to_string()),
        Some("2099-12-31".to_string()),
        None,
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
        None,
        false,
        false,
        &config,
    )
    .await?;
    assert_eq!(included.len(), 1);
    assert_eq!(
        included[0].annotation.as_ref().unwrap().ignore_spending,
        Some(true)
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

    let out = list_transactions(
        &storage,
        Some("2000-01-01".to_string()),
        Some("2099-12-31".to_string()),
        None,
        true,
        true,
        &ResolvedConfig {
            data_dir: std::path::PathBuf::from("/tmp"),
            reporting_currency: "USD".to_string(),
            display: crate::config::DisplayConfig::default(),
            refresh: crate::config::RefreshConfig::default(),
            history: crate::config::HistoryConfig::default(),
            tray: crate::config::TrayConfig::default(),
            spending: crate::config::SpendingConfig::default(),
            tags: Default::default(),
            portfolio: crate::config::PortfolioConfig::default(),
            ignore: crate::config::IgnoreConfig::default(),
            ai: crate::config::AiConfig::default(),
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
        history: crate::config::HistoryConfig::default(),
        tray: crate::config::TrayConfig::default(),
        spending: crate::config::SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
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
        ai: crate::config::AiConfig::default(),
        git: crate::config::GitConfig::default(),
    };

    let skipped = list_transactions(
        &storage,
        Some("2000-01-01".to_string()),
        Some("2099-12-31".to_string()),
        None,
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
        None,
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
        history: crate::config::HistoryConfig::default(),
        tray: crate::config::TrayConfig::default(),
        spending: crate::config::SpendingConfig {
            ignore_accounts: vec!["Investor Checking".to_string()],
            ignore_connections: vec![],
            ignore_tags: vec![],
        },
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: crate::config::GitConfig::default(),
    };

    let skipped = list_transactions(
        &storage,
        Some("2000-01-01".to_string()),
        Some("2099-12-31".to_string()),
        None,
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
        None,
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
        history: crate::config::HistoryConfig::default(),
        tray: crate::config::TrayConfig::default(),
        spending: crate::config::SpendingConfig {
            ignore_accounts: vec![],
            ignore_connections: vec![],
            ignore_tags: vec!["brokerage".to_string()],
        },
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: crate::config::GitConfig::default(),
    };

    let skipped = list_transactions(
        &storage,
        Some("2000-01-01".to_string()),
        Some("2099-12-31".to_string()),
        None,
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
        None,
        false,
        false,
        &config,
    )
    .await?;
    assert_eq!(included.len(), 1);
    Ok(())
}

#[tokio::test]
async fn list_transactions_ignores_internal_transfer_hints_when_skipping_ignored() -> Result<()> {
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
        history: crate::config::HistoryConfig::default(),
        tray: crate::config::TrayConfig::default(),
        spending: crate::config::SpendingConfig::default(),
        tags: Default::default(),
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: crate::config::GitConfig::default(),
    };

    let skipped = list_transactions(
        &storage,
        Some("2000-01-01".to_string()),
        Some("2099-12-31".to_string()),
        None,
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
        None,
        false,
        false,
        &config,
    )
    .await?;
    assert_eq!(included.len(), 1);
    Ok(())
}

use super::*;
use crate::clock::Clock;
use crate::clock::FixedClock;
use crate::market_data::{FxRateKind, FxRatePoint, MemoryMarketDataStore, PriceKind, PricePoint};
use crate::models::{
    Account, Asset, FixedIdGenerator, Transaction, TransactionAnnotationPatch,
    TransactionStandardizedMetadata,
};
use crate::storage::MemoryStorage;
use chrono::TimeZone;

#[tokio::test]
async fn spending_report_supports_end_bound_monthly_alignment() -> Result<()> {
    let storage = MemoryStorage::new();
    let conn_id = Id::from_string("conn-1");
    let acct_id = Id::from_string("acct-1");
    let account = Account::new_with(acct_id.clone(), Utc::now(), "Checking", conn_id);
    storage.save_account(&account).await?;

    let ids = FixedIdGenerator::new([
        Id::from_string("tx-jan-15"),
        Id::from_string("tx-jan-26"),
        Id::from_string("tx-feb-25"),
        Id::from_string("tx-feb-26"),
        Id::from_string("tx-mar-25"),
        Id::from_string("tx-mar-26"),
        Id::from_string("tx-apr-25"),
    ]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
    let tx = |date: (i32, u32, u32), amount: &str, description: &str| {
        Transaction::new_with_generator(&ids, &clock, amount, Asset::currency("USD"), description)
            .with_timestamp(
                Utc.with_ymd_and_hms(date.0, date.1, date.2, 12, 0, 0)
                    .unwrap(),
            )
    };
    storage
        .append_transactions(
            &acct_id,
            &[
                tx((2026, 1, 15), "-10", "Jan early"),
                tx((2026, 1, 26), "-20", "Jan trailing"),
                tx((2026, 2, 25), "-30", "Feb trailing"),
                tx((2026, 2, 26), "-40", "Feb next"),
                tx((2026, 3, 25), "-50", "Mar trailing"),
                tx((2026, 3, 26), "-60", "Mar next"),
                tx((2026, 4, 25), "-70", "Apr trailing"),
            ],
        )
        .await?;

    let cfg = ResolvedConfig {
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

    let out = spending_report_with_store(
        &storage,
        &cfg,
        SpendingReportOptions {
            currency: None,
            start: Some("2026-01-10".to_string()),
            end: Some("2026-04-25".to_string()),
            period: "monthly".to_string(),
            period_alignment: Some("end-bound".to_string()),
            tz: Some("UTC".to_string()),
            week_start: None,
            bucket: None,
            account: Some("acct-1".to_string()),
            connection: None,
            status: "posted".to_string(),
            direction: "outflow".to_string(),
            group_by: "none".to_string(),
            top: None,
            lookback_days: 7,
            include_noncurrency: false,
            include_empty: true,
        },
        Arc::new(MemoryMarketDataStore::default()),
    )
    .await?;

    assert_eq!(out.period_alignment, "end-bound");
    let periods: Vec<_> = out
        .periods
        .iter()
        .map(|p| {
            (
                p.start_date.as_str(),
                p.end_date.as_str(),
                p.total.as_str(),
                p.transaction_count,
            )
        })
        .collect();
    assert_eq!(
        periods,
        vec![
            ("2026-01-10", "2026-01-25", "10", 1),
            ("2026-01-26", "2026-02-25", "50", 2),
            ("2026-02-26", "2026-03-25", "90", 2),
            ("2026-03-26", "2026-04-25", "130", 2),
        ]
    );

    Ok(())
}

#[tokio::test]
async fn spending_report_buckets_by_timezone_date() -> Result<()> {
    let storage = MemoryStorage::new();
    let conn_id = Id::from_string("conn-1");
    let acct_id = Id::from_string("acct-1");
    let account = Account::new_with(acct_id.clone(), Utc::now(), "Checking", conn_id);
    storage.save_account(&account).await?;

    // 2026-02-01T02:30Z is 2026-01-31 in America/New_York (UTC-05 in winter).
    let tx_id_gen = FixedIdGenerator::new([Id::from_string("tx-1")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 1, 2, 30, 0).unwrap());
    let tx =
        Transaction::new_with_generator(&tx_id_gen, &clock, "-10", Asset::currency("USD"), "Test")
            .with_timestamp(clock.now());
    storage.append_transactions(&acct_id, &[tx]).await?;

    let store = Arc::new(MemoryMarketDataStore::default());
    let cfg = ResolvedConfig {
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

    let out = spending_report_with_store(
        &storage,
        &cfg,
        SpendingReportOptions {
            currency: None,
            start: Some("2026-01-30".to_string()),
            end: Some("2026-02-02".to_string()),
            period: "daily".to_string(),
            period_alignment: None,
            tz: Some("America/New_York".to_string()),
            week_start: None,
            bucket: None,
            account: Some("acct-1".to_string()),
            connection: None,
            status: "posted".to_string(),
            direction: "outflow".to_string(),
            group_by: "none".to_string(),
            top: None,
            lookback_days: 7,
            include_noncurrency: false,
            include_empty: false,
        },
        store,
    )
    .await?;

    assert_eq!(out.periods.len(), 1);
    assert_eq!(out.periods[0].start_date, "2026-01-31");
    assert_eq!(out.periods[0].total, "10");
    Ok(())
}

#[tokio::test]
async fn spending_report_converts_fx_and_prices() -> Result<()> {
    let storage = MemoryStorage::new();
    let conn_id = Id::from_string("conn-1");
    let acct_id = Id::from_string("acct-1");
    let account = Account::new_with(acct_id.clone(), Utc::now(), "Checking", conn_id);
    storage.save_account(&account).await?;

    let ids = FixedIdGenerator::new([Id::from_string("tx-eur"), Id::from_string("tx-eq")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

    let tx_eur =
        Transaction::new_with_generator(&ids, &clock, "-10", Asset::currency("EUR"), "EUR debit")
            .with_timestamp(clock.now());
    let tx_eq = Transaction::new_with_generator(
        &ids,
        &clock,
        "-2",
        Asset::equity("AAPL"),
        "Buy AAPL shares",
    )
    .with_timestamp(clock.now());
    storage
        .append_transactions(&acct_id, &[tx_eur, tx_eq])
        .await?;

    let store = Arc::new(MemoryMarketDataStore::default());
    // EURUSD close 1.2 on 2026-02-05 => -10 EUR -> -12 USD (outflow 12).
    store
        .put_fx_rates(&[FxRatePoint {
            base: "EUR".to_string(),
            quote: "USD".to_string(),
            as_of_date: NaiveDate::from_ymd_opt(2026, 2, 5).unwrap(),
            timestamp: clock.now(),
            rate: "1.2".to_string(),
            kind: FxRateKind::Close,
            source: "test".to_string(),
        }])
        .await?;
    // AAPL close 50 USD on 2026-02-05 => -2 shares -> -100 USD (outflow 100).
    store
        .put_prices(&[PricePoint {
            asset_id: crate::market_data::AssetId::from_asset(&Asset::equity("AAPL").normalized()),
            as_of_date: NaiveDate::from_ymd_opt(2026, 2, 5).unwrap(),
            timestamp: clock.now(),
            price: "50".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }])
        .await?;

    let cfg = ResolvedConfig {
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

    let out = spending_report_with_store(
        &storage,
        &cfg,
        SpendingReportOptions {
            currency: None,
            start: Some("2026-02-01".to_string()),
            end: Some("2026-02-28".to_string()),
            period: "monthly".to_string(),
            period_alignment: None,
            tz: Some("UTC".to_string()),
            week_start: None,
            bucket: None,
            account: Some("acct-1".to_string()),
            connection: None,
            status: "posted".to_string(),
            direction: "outflow".to_string(),
            group_by: "none".to_string(),
            top: None,
            lookback_days: 7,
            include_noncurrency: true,
            include_empty: false,
        },
        store,
    )
    .await?;

    assert_eq!(out.total, "112");
    assert_eq!(out.transaction_count, 2);
    Ok(())
}

#[tokio::test]
async fn spending_report_tag_uses_annotation_then_metadata_then_untagged() -> Result<()> {
    let storage = MemoryStorage::new();
    let conn_id = Id::from_string("conn-1");
    let acct_id = Id::from_string("acct-1");
    let account = Account::new_with(acct_id.clone(), Utc::now(), "Checking", conn_id);
    storage.save_account(&account).await?;

    let ids = FixedIdGenerator::new([Id::from_string("tx-meta"), Id::from_string("tx-ann")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());

    let tx_meta = Transaction::new_with_generator(
        &ids,
        &clock,
        "-10",
        Asset::currency("USD"),
        "Fallback to provider tag",
    )
    .with_timestamp(clock.now())
    .with_standardized_metadata(TransactionStandardizedMetadata {
        merchant_category_label: Some("Groceries".to_string()),
        ..Default::default()
    });
    let tx_ann = Transaction::new_with_generator(
        &ids,
        &clock,
        "-20",
        Asset::currency("USD"),
        "Annotation tag wins",
    )
    .with_timestamp(clock.now())
    .with_standardized_metadata(TransactionStandardizedMetadata {
        merchant_category_label: Some("Shopping".to_string()),
        ..Default::default()
    });
    let tx_ann_id = tx_ann.id.clone();
    storage
        .append_transactions(&acct_id, &[tx_meta, tx_ann])
        .await?;
    storage
        .append_transaction_annotation_patches(
            &acct_id,
            &[TransactionAnnotationPatch {
                transaction_id: tx_ann_id,
                timestamp: clock.now(),
                description: None,
                note: None,
                tags: Some(Some(vec!["Dining".to_string()])),
                subtags: Some(Some(vec!["Restaurants".to_string()])),
                effective_date: None,
            }],
        )
        .await?;

    let cfg = ResolvedConfig {
        data_dir: std::path::PathBuf::from("/tmp"),
        reporting_currency: "USD".to_string(),
        display: crate::config::DisplayConfig::default(),
        refresh: crate::config::RefreshConfig::default(),
        history: crate::config::HistoryConfig::default(),
        tray: crate::config::TrayConfig::default(),
        spending: crate::config::SpendingConfig::default(),
        tags: crate::config::TagsConfig {
            aliases: HashMap::new(),
            parents: HashMap::from([("Groceries".to_string(), vec!["Food".to_string()])]),
        },
        portfolio: crate::config::PortfolioConfig::default(),
        ignore: crate::config::IgnoreConfig::default(),
        ai: crate::config::AiConfig::default(),
        git: crate::config::GitConfig::default(),
    };

    let out = spending_report_with_store(
        &storage,
        &cfg,
        SpendingReportOptions {
            currency: None,
            start: Some("2026-02-01".to_string()),
            end: Some("2026-02-28".to_string()),
            period: "monthly".to_string(),
            period_alignment: None,
            tz: Some("UTC".to_string()),
            week_start: None,
            bucket: None,
            account: Some("acct-1".to_string()),
            connection: None,
            status: "posted".to_string(),
            direction: "outflow".to_string(),
            group_by: "tag".to_string(),
            top: None,
            lookback_days: 7,
            include_noncurrency: false,
            include_empty: false,
        },
        Arc::new(MemoryMarketDataStore::default()),
    )
    .await?;

    assert_eq!(out.total, "30");
    assert_eq!(out.transaction_count, 2);
    assert_eq!(out.periods.len(), 1);
    assert_eq!(out.periods[0].breakdown.len(), 2);
    assert_eq!(out.periods[0].breakdown[0].key, "Dining");
    assert_eq!(out.periods[0].breakdown[0].total, "20");
    assert_eq!(out.periods[0].breakdown[1].key, "Food");
    assert_eq!(out.periods[0].breakdown[1].total, "10");

    let subtag_out = spending_report_with_store(
        &storage,
        &cfg,
        SpendingReportOptions {
            currency: None,
            start: Some("2026-02-01".to_string()),
            end: Some("2026-02-28".to_string()),
            period: "monthly".to_string(),
            period_alignment: None,
            tz: Some("UTC".to_string()),
            week_start: None,
            bucket: None,
            account: Some("acct-1".to_string()),
            connection: None,
            status: "posted".to_string(),
            direction: "outflow".to_string(),
            group_by: "subtag".to_string(),
            top: None,
            lookback_days: 7,
            include_noncurrency: false,
            include_empty: false,
        },
        Arc::new(MemoryMarketDataStore::default()),
    )
    .await?;
    assert_eq!(subtag_out.periods[0].breakdown[0].key, "Restaurants");
    assert_eq!(subtag_out.periods[0].breakdown[0].total, "20");
    assert_eq!(subtag_out.periods[0].breakdown[1].key, "Groceries");
    assert_eq!(subtag_out.periods[0].breakdown[1].total, "10");
    Ok(())
}

#[tokio::test]
async fn spending_report_supports_exact_and_close_merchant_grouping() -> Result<()> {
    let storage = MemoryStorage::new();
    let conn_id = Id::from_string("conn-1");
    let acct_id = Id::from_string("acct-1");
    let account = Account::new_with(acct_id.clone(), Utc::now(), "Checking", conn_id);
    storage.save_account(&account).await?;

    let ids = FixedIdGenerator::new([
        Id::from_string("tx-coffee-100"),
        Id::from_string("tx-coffee-200"),
        Id::from_string("tx-grocery"),
    ]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());
    let tx = |amount: &str, description: &str| {
        Transaction::new_with_generator(&ids, &clock, amount, Asset::currency("USD"), description)
            .with_timestamp(clock.now())
    };
    storage
        .append_transactions(
            &acct_id,
            &[
                tx("-10", "Coffee Shop #100"),
                tx("-15", "Coffee Shop #200"),
                tx("-20", "Grocery Market #9"),
            ],
        )
        .await?;

    let cfg = ResolvedConfig {
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

    let exact = spending_report_with_store(
        &storage,
        &cfg,
        SpendingReportOptions {
            currency: None,
            start: Some("2026-02-01".to_string()),
            end: Some("2026-02-28".to_string()),
            period: "range".to_string(),
            period_alignment: None,
            tz: Some("UTC".to_string()),
            week_start: None,
            bucket: None,
            account: Some("acct-1".to_string()),
            connection: None,
            status: "posted".to_string(),
            direction: "outflow".to_string(),
            group_by: "merchant".to_string(),
            top: None,
            lookback_days: 7,
            include_noncurrency: false,
            include_empty: false,
        },
        Arc::new(MemoryMarketDataStore::default()),
    )
    .await?;
    assert_eq!(exact.periods[0].breakdown.len(), 3);
    assert!(exact.periods[0]
        .breakdown
        .iter()
        .any(|entry| entry.key == "Coffee Shop #100" && entry.total == "10"));
    assert!(exact.periods[0]
        .breakdown
        .iter()
        .any(|entry| entry.key == "Coffee Shop #200" && entry.total == "15"));

    let close = spending_report_with_store(
        &storage,
        &cfg,
        SpendingReportOptions {
            currency: None,
            start: Some("2026-02-01".to_string()),
            end: Some("2026-02-28".to_string()),
            period: "range".to_string(),
            period_alignment: None,
            tz: Some("UTC".to_string()),
            week_start: None,
            bucket: None,
            account: Some("acct-1".to_string()),
            connection: None,
            status: "posted".to_string(),
            direction: "outflow".to_string(),
            group_by: "merchant_fuzzy".to_string(),
            top: None,
            lookback_days: 7,
            include_noncurrency: false,
            include_empty: false,
        },
        Arc::new(MemoryMarketDataStore::default()),
    )
    .await?;
    assert_eq!(close.periods[0].breakdown.len(), 2);
    assert_eq!(close.periods[0].breakdown[0].key, "coffee shop");
    assert_eq!(close.periods[0].breakdown[0].total, "25");
    assert_eq!(close.periods[0].breakdown[0].transaction_count, 2);
    assert_eq!(close.periods[0].breakdown[1].key, "grocery market");
    assert_eq!(close.periods[0].breakdown[1].total, "20");
    Ok(())
}

#[tokio::test]
async fn spending_report_ignores_accounts_by_configured_tags() -> Result<()> {
    let storage = MemoryStorage::new();
    let conn_id = Id::from_string("conn-1");

    let acct_card_id = Id::from_string("acct-card");
    let card = Account::new_with(acct_card_id.clone(), Utc::now(), "Card", conn_id.clone());
    storage.save_account(&card).await?;

    let acct_brokerage_id = Id::from_string("acct-brokerage");
    let mut brokerage = Account::new_with(
        acct_brokerage_id.clone(),
        Utc::now(),
        "Individual",
        conn_id.clone(),
    );
    brokerage.tags = vec!["brokerage".to_string()];
    storage.save_account(&brokerage).await?;

    let ids = FixedIdGenerator::new([Id::from_string("tx-card"), Id::from_string("tx-brokerage")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());
    let tx_card =
        Transaction::new_with_generator(&ids, &clock, "-10", Asset::currency("USD"), "Card spend")
            .with_timestamp(clock.now());
    let tx_brokerage = Transaction::new_with_generator(
        &ids,
        &clock,
        "-2000",
        Asset::currency("USD"),
        "Brokerage transfer",
    )
    .with_timestamp(clock.now());
    storage
        .append_transactions(&acct_card_id, &[tx_card])
        .await?;
    storage
        .append_transactions(&acct_brokerage_id, &[tx_brokerage])
        .await?;

    let cfg = ResolvedConfig {
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

    let out = spending_report_with_store(
        &storage,
        &cfg,
        SpendingReportOptions {
            currency: None,
            start: Some("2026-02-01".to_string()),
            end: Some("2026-02-28".to_string()),
            period: "monthly".to_string(),
            period_alignment: None,
            tz: Some("UTC".to_string()),
            week_start: None,
            bucket: None,
            account: None,
            connection: None,
            status: "posted".to_string(),
            direction: "outflow".to_string(),
            group_by: "none".to_string(),
            top: None,
            lookback_days: 7,
            include_noncurrency: false,
            include_empty: false,
        },
        Arc::new(MemoryMarketDataStore::default()),
    )
    .await?;

    assert_eq!(out.total, "10");
    assert_eq!(out.transaction_count, 1);
    Ok(())
}

#[tokio::test]
async fn spending_report_ignores_transactions_by_global_regex_rules() -> Result<()> {
    let storage = MemoryStorage::new();
    let connection = crate::models::Connection::new(crate::models::ConnectionConfig {
        name: "Charles Schwab".to_string(),
        synchronizer: "schwab".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    let conn_id = connection.id().clone();
    storage.save_connection(&connection).await?;

    let acct_id = Id::from_string("acct-checking");
    let account = Account::new_with(
        acct_id.clone(),
        Utc::now(),
        "Investor Checking",
        conn_id.clone(),
    );
    storage.save_account(&account).await?;

    let ids = FixedIdGenerator::new([Id::from_string("tx-cc"), Id::from_string("tx-rent")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap());
    let tx_cc = Transaction::new_with_generator(
        &ids,
        &clock,
        "-1000",
        Asset::currency("USD"),
        "ACH CHASE CREDIT CRD EPAY 260217",
    )
    .with_timestamp(clock.now());
    let tx_rent = Transaction::new_with_generator(
        &ids,
        &clock,
        "-2000",
        Asset::currency("USD"),
        "ACH BALLAST-CZB-6708 WEB PMTS 012626",
    )
    .with_timestamp(clock.now());
    storage
        .append_transactions(&acct_id, &[tx_cc, tx_rent])
        .await?;

    let cfg = ResolvedConfig {
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
                account_name: Some("(?i)^Investor Checking$".to_string()),
                connection_name: Some("(?i)^Charles Schwab$".to_string()),
                synchronizer: Some("(?i)^schwab$".to_string()),
                description: Some("(?i)credit\\s+crd\\s+(?:e?pay|autopay)".to_string()),
                ..Default::default()
            }],
        },
        ai: crate::config::AiConfig::default(),
        git: crate::config::GitConfig::default(),
    };

    let out = spending_report_with_store(
        &storage,
        &cfg,
        SpendingReportOptions {
            currency: None,
            start: Some("2026-02-01".to_string()),
            end: Some("2026-02-28".to_string()),
            period: "monthly".to_string(),
            period_alignment: None,
            tz: Some("UTC".to_string()),
            week_start: None,
            bucket: None,
            account: Some("acct-checking".to_string()),
            connection: None,
            status: "posted".to_string(),
            direction: "outflow".to_string(),
            group_by: "none".to_string(),
            top: None,
            lookback_days: 7,
            include_noncurrency: false,
            include_empty: false,
        },
        Arc::new(MemoryMarketDataStore::default()),
    )
    .await?;

    assert_eq!(out.total, "2000");
    assert_eq!(out.transaction_count, 1);
    Ok(())
}

#[tokio::test]
async fn spending_report_ignores_internal_transfer_hints() -> Result<()> {
    let storage = MemoryStorage::new();
    let conn_id = Id::from_string("conn-1");
    let acct_id = Id::from_string("acct-1");
    let account = Account::new_with(
        acct_id.clone(),
        Utc::now(),
        "Sapphire Reserve (6395)",
        conn_id,
    );
    storage.save_account(&account).await?;

    let ids = FixedIdGenerator::new([Id::from_string("tx-pay"), Id::from_string("tx-food")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 18, 12, 0, 0).unwrap());
    let tx_payment = Transaction::new_with_generator(
        &ids,
        &clock,
        "-4450.62",
        Asset::currency("USD"),
        "Payment Thank You - Web",
    )
    .with_standardized_metadata(TransactionStandardizedMetadata {
        transaction_kind: Some("payment".to_string()),
        is_internal_transfer_hint: Some(true),
        ..TransactionStandardizedMetadata::default()
    })
    .with_timestamp(clock.now());
    let tx_food = Transaction::new_with_generator(
        &ids,
        &clock,
        "-25",
        Asset::currency("USD"),
        "Bay Padel LLC",
    )
    .with_timestamp(clock.now());
    storage
        .append_transactions(&acct_id, &[tx_payment, tx_food])
        .await?;

    let cfg = ResolvedConfig {
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

    let out = spending_report_with_store(
        &storage,
        &cfg,
        SpendingReportOptions {
            currency: None,
            start: Some("2026-02-01".to_string()),
            end: Some("2026-02-28".to_string()),
            period: "monthly".to_string(),
            period_alignment: None,
            tz: Some("UTC".to_string()),
            week_start: None,
            bucket: None,
            account: Some("acct-1".to_string()),
            connection: None,
            status: "posted".to_string(),
            direction: "outflow".to_string(),
            group_by: "none".to_string(),
            top: None,
            lookback_days: 7,
            include_noncurrency: false,
            include_empty: false,
        },
        Arc::new(MemoryMarketDataStore::default()),
    )
    .await?;

    assert_eq!(out.total, "25");
    assert_eq!(out.transaction_count, 1);
    Ok(())
}

#[tokio::test]
async fn spending_report_ignores_transactions_marked_ignore_spending_tag() -> Result<()> {
    let storage = MemoryStorage::new();
    let conn_id = Id::from_string("conn-1");
    let acct_id = Id::from_string("acct-1");
    let account = Account::new_with(acct_id.clone(), Utc::now(), "Checking", conn_id);
    storage.save_account(&account).await?;

    let ids = FixedIdGenerator::new([Id::from_string("tx-keep"), Id::from_string("tx-skip")]);
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 2, 18, 12, 0, 0).unwrap());
    let tx_keep =
        Transaction::new_with_generator(&ids, &clock, "-25", Asset::currency("USD"), "Coffee")
            .with_timestamp(clock.now());
    let tx_skip = Transaction::new_with_generator(
        &ids,
        &clock,
        "-30000",
        Asset::currency("USD"),
        "WIRE Outgoing Wire",
    )
    .with_timestamp(clock.now());
    let tx_skip_id = tx_skip.id.clone();
    storage
        .append_transactions(&acct_id, &[tx_keep, tx_skip])
        .await?;
    storage
        .append_transaction_annotation_patches(
            &acct_id,
            &[TransactionAnnotationPatch {
                transaction_id: tx_skip_id,
                timestamp: clock.now(),
                description: None,
                note: None,
                tags: Some(Some(vec!["ignore_spending".to_string()])),
                subtags: None,
                effective_date: None,
            }],
        )
        .await?;

    let cfg = ResolvedConfig {
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

    let out = spending_report_with_store(
        &storage,
        &cfg,
        SpendingReportOptions {
            currency: None,
            start: Some("2026-02-01".to_string()),
            end: Some("2026-02-28".to_string()),
            period: "monthly".to_string(),
            period_alignment: None,
            tz: Some("UTC".to_string()),
            week_start: None,
            bucket: None,
            account: Some("acct-1".to_string()),
            connection: None,
            status: "posted".to_string(),
            direction: "outflow".to_string(),
            group_by: "none".to_string(),
            top: None,
            lookback_days: 7,
            include_noncurrency: false,
            include_empty: false,
        },
        Arc::new(MemoryMarketDataStore::default()),
    )
    .await?;

    assert_eq!(out.total, "25");
    assert_eq!(out.transaction_count, 1);
    Ok(())
}

use std::path::Path;

use anyhow::Result;
use chrono::TimeZone;
use keepbook::app::{
    add_transaction_rule, apply_transaction_rules, list_transaction_rules, list_transactions,
    ApplyTransactionRulesOptions, TransactionRule,
};
use keepbook::config::{
    AiConfig, DisplayConfig, GitConfig, HistoryConfig, IgnoreConfig, PortfolioConfig,
    RefreshConfig, ResolvedConfig, SpendingConfig, TrayConfig,
};
use keepbook::models::{Account, Asset, Id, Transaction, TransactionStandardizedMetadata};
use keepbook::storage::{JsonFileStorage, Storage};

fn resolved_config(data_dir: &Path) -> ResolvedConfig {
    ResolvedConfig {
        data_dir: data_dir.to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        portfolio: PortfolioConfig::default(),
        ignore: IgnoreConfig::default(),
        ai: AiConfig::default(),
        git: GitConfig::default(),
    }
}

#[tokio::test]
async fn transaction_rules_append_and_apply_to_existing_transactions() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = resolved_config(dir.path());

    let mut account = Account::new("Checking", Id::from_string("conn-1"));
    account.id = Id::from_string("acct-1");
    storage.save_account(&account).await?;

    let coffee = Transaction::new("-4.50", Asset::currency("USD"), "Blue Bottle Coffee")
        .with_id(Id::from_string("tx-coffee"))
        .with_timestamp(chrono::Utc.with_ymd_and_hms(2026, 2, 10, 12, 0, 0).unwrap());
    let grocery = Transaction::new("-25.00", Asset::currency("USD"), "Grocery Store")
        .with_id(Id::from_string("tx-grocery"))
        .with_timestamp(chrono::Utc.with_ymd_and_hms(2026, 2, 11, 12, 0, 0).unwrap());
    storage
        .append_transactions(&account.id, &[coffee.clone(), grocery])
        .await?;

    let add_output = add_transaction_rule(
        &config,
        TransactionRule {
            set_category: Some("Coffee".to_string()),
            set_subcategory: None,
            set_description: Some("Blue Bottle".to_string()),
            set_tags: None,
            match_account_id: None,
            match_account_name: Some("(?i)^checking$".to_string()),
            match_description: Some("(?i)coffee".to_string()),
            match_category: None,
            match_subcategory: None,
            match_status: None,
            match_amount: None,
        },
    )
    .await?;
    assert_eq!(add_output["success"], true);

    let listed = list_transaction_rules(&config)?;
    assert_eq!(listed["rule_count"], 1);
    assert_eq!(listed["invalid_rule_count"], 0);

    let dry_run = apply_transaction_rules(
        &storage,
        &config,
        ApplyTransactionRulesOptions {
            start: Some("2026-02-01".to_string()),
            end: Some("2026-02-28".to_string()),
            account: None,
            connection: None,
            overwrite: false,
            dry_run: true,
        },
    )
    .await?;
    assert_eq!(dry_run["matched_count"], 1);
    assert_eq!(dry_run["updated_count"], 1);
    assert_eq!(
        storage
            .get_transaction_annotation_patches(&account.id)
            .await?
            .len(),
        0
    );

    let applied = apply_transaction_rules(
        &storage,
        &config,
        ApplyTransactionRulesOptions {
            start: Some("2026-02-01".to_string()),
            end: Some("2026-02-28".to_string()),
            account: None,
            connection: None,
            overwrite: false,
            dry_run: false,
        },
    )
    .await?;
    assert_eq!(applied["matched_count"], 1);
    assert_eq!(applied["updated_count"], 1);

    let transactions = list_transactions(
        &storage,
        Some("2026-02-01".to_string()),
        Some("2026-02-28".to_string()),
        None,
        false,
        false,
        &config,
    )
    .await?;
    let categorized = transactions
        .iter()
        .find(|tx| tx.id == coffee.id.to_string())
        .expect("coffee transaction should be listed");
    assert_eq!(categorized.category.as_deref(), Some("Coffee"));
    assert_eq!(
        categorized
            .annotation
            .as_ref()
            .and_then(|annotation| annotation.description.as_deref()),
        Some("Blue Bottle")
    );

    Ok(())
}

#[tokio::test]
async fn transaction_rules_do_not_overwrite_existing_annotations_by_default() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = resolved_config(dir.path());

    let mut account = Account::new("Checking", Id::from_string("conn-1"));
    account.id = Id::from_string("acct-1");
    storage.save_account(&account).await?;

    let lunch = Transaction::new("-12.00", Asset::currency("USD"), "Lunch Spot")
        .with_id(Id::from_string("tx-lunch"))
        .with_timestamp(chrono::Utc.with_ymd_and_hms(2026, 2, 12, 12, 0, 0).unwrap());
    storage
        .append_transactions(&account.id, std::slice::from_ref(&lunch))
        .await?;

    keepbook::app::set_transaction_categories(
        &storage,
        &config,
        vec![(account.id.to_string(), lunch.id.to_string())],
        Some("Manual".to_string()),
        false,
    )
    .await?;

    add_transaction_rule(
        &config,
        TransactionRule {
            set_category: Some("Dining".to_string()),
            set_subcategory: None,
            set_description: Some("Lunch".to_string()),
            set_tags: None,
            match_account_id: None,
            match_account_name: None,
            match_description: Some("(?i)lunch".to_string()),
            match_category: None,
            match_subcategory: None,
            match_status: None,
            match_amount: None,
        },
    )
    .await?;

    let skipped = apply_transaction_rules(
        &storage,
        &config,
        ApplyTransactionRulesOptions {
            start: None,
            end: None,
            account: None,
            connection: None,
            overwrite: false,
            dry_run: false,
        },
    )
    .await?;
    assert_eq!(skipped["matched_count"], 1);
    assert_eq!(skipped["updated_count"], 1);
    assert_eq!(skipped["skipped_existing_action_count"], 0);

    let transactions = list_transactions(
        &storage,
        Some("2026-02-01".to_string()),
        Some("2026-02-28".to_string()),
        None,
        false,
        false,
        &config,
    )
    .await?;
    let categorized = transactions
        .iter()
        .find(|tx| tx.id == lunch.id.to_string())
        .expect("lunch transaction should be listed");
    assert_eq!(categorized.category.as_deref(), Some("Manual"));
    assert_eq!(
        categorized
            .annotation
            .as_ref()
            .and_then(|annotation| annotation.description.as_deref()),
        Some("Lunch")
    );

    let overwritten = apply_transaction_rules(
        &storage,
        &config,
        ApplyTransactionRulesOptions {
            start: None,
            end: None,
            account: None,
            connection: None,
            overwrite: true,
            dry_run: false,
        },
    )
    .await?;
    assert_eq!(overwritten["updated_count"], 1);

    let transactions = list_transactions(
        &storage,
        Some("2026-02-01".to_string()),
        Some("2026-02-28".to_string()),
        None,
        false,
        false,
        &config,
    )
    .await?;
    let overwritten_tx = transactions
        .iter()
        .find(|tx| tx.id == lunch.id.to_string())
        .expect("lunch transaction should be listed");
    assert_eq!(overwritten_tx.category.as_deref(), Some("Dining"));

    Ok(())
}

#[tokio::test]
async fn transaction_rules_can_remap_metadata_category_and_set_subcategory() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let storage = JsonFileStorage::new(dir.path());
    let config = resolved_config(dir.path());

    let mut account = Account::new("Checking", Id::from_string("conn-1"));
    account.id = Id::from_string("acct-1");
    storage.save_account(&account).await?;

    let grocery = Transaction::new("-45.00", Asset::currency("USD"), "Corner Market")
        .with_id(Id::from_string("tx-grocery"))
        .with_timestamp(chrono::Utc.with_ymd_and_hms(2026, 2, 13, 12, 0, 0).unwrap())
        .with_standardized_metadata(TransactionStandardizedMetadata {
            merchant_category_label: Some("Groceries".to_string()),
            ..TransactionStandardizedMetadata::default()
        });
    let restaurant = Transaction::new("-30.00", Asset::currency("USD"), "Dinner")
        .with_id(Id::from_string("tx-restaurant"))
        .with_timestamp(chrono::Utc.with_ymd_and_hms(2026, 2, 13, 13, 0, 0).unwrap())
        .with_standardized_metadata(TransactionStandardizedMetadata {
            merchant_category_label: Some("Food and Drink".to_string()),
            ..TransactionStandardizedMetadata::default()
        });
    storage
        .append_transactions(&account.id, &[grocery.clone(), restaurant.clone()])
        .await?;

    add_transaction_rule(
        &config,
        TransactionRule {
            set_category: Some("Food".to_string()),
            set_subcategory: Some("Groceries".to_string()),
            set_description: None,
            set_tags: Some(vec!["Food".to_string(), "Groceries".to_string()]),
            match_account_id: None,
            match_account_name: None,
            match_description: None,
            match_category: Some("(?i)^groceries$".to_string()),
            match_subcategory: None,
            match_status: None,
            match_amount: None,
        },
    )
    .await?;
    add_transaction_rule(
        &config,
        TransactionRule {
            set_category: Some("Food".to_string()),
            set_subcategory: None,
            set_description: None,
            set_tags: Some(vec!["Food".to_string()]),
            match_account_id: None,
            match_account_name: None,
            match_description: None,
            match_category: Some("(?i)^food and drink$".to_string()),
            match_subcategory: None,
            match_status: None,
            match_amount: None,
        },
    )
    .await?;

    let applied = apply_transaction_rules(
        &storage,
        &config,
        ApplyTransactionRulesOptions {
            start: None,
            end: None,
            account: None,
            connection: None,
            overwrite: false,
            dry_run: false,
        },
    )
    .await?;
    assert_eq!(applied["matched_count"], 2);
    assert_eq!(applied["updated_count"], 2);

    let transactions = list_transactions(
        &storage,
        Some("2026-02-01".to_string()),
        Some("2026-02-28".to_string()),
        None,
        false,
        false,
        &config,
    )
    .await?;
    let grocery_out = transactions
        .iter()
        .find(|tx| tx.id == grocery.id.to_string())
        .expect("grocery transaction should be listed");
    assert_eq!(grocery_out.category.as_deref(), Some("Food"));
    assert_eq!(grocery_out.subcategory.as_deref(), Some("Groceries"));
    assert_eq!(
        grocery_out.tags,
        vec!["Food".to_string(), "Groceries".to_string()]
    );

    let restaurant_out = transactions
        .iter()
        .find(|tx| tx.id == restaurant.id.to_string())
        .expect("restaurant transaction should be listed");
    assert_eq!(restaurant_out.category.as_deref(), Some("Food"));
    assert_eq!(restaurant_out.subcategory.as_deref(), None);
    assert_eq!(restaurant_out.tags, vec!["Food".to_string()]);

    Ok(())
}

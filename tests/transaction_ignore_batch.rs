use std::path::Path;

use anyhow::Result;
use keepbook::app::set_transaction_ignore;
use keepbook::config::{
    AiConfig, DisplayConfig, GitConfig, HistoryConfig, IgnoreConfig, PortfolioConfig,
    RefreshConfig, ResolvedConfig, SpendingConfig, TrayConfig,
};
use keepbook::models::{Account, Asset, Transaction, TransactionAnnotation};
use keepbook::storage::{MemoryStorage, Storage};

fn resolved_config(data_dir: &Path) -> ResolvedConfig {
    ResolvedConfig {
        data_dir: data_dir.to_path_buf(),
        reporting_currency: "USD".to_string(),
        display: DisplayConfig::default(),
        refresh: RefreshConfig::default(),
        history: HistoryConfig::default(),
        tray: TrayConfig::default(),
        spending: SpendingConfig::default(),
        tags: Default::default(),
        portfolio: PortfolioConfig::default(),
        ignore: IgnoreConfig::default(),
        ai: AiConfig::default(),
        git: GitConfig::default(),
    }
}

async fn materialized_annotation(
    storage: &MemoryStorage,
    account_id: &keepbook::models::Id,
    transaction_id: &keepbook::models::Id,
) -> Result<TransactionAnnotation> {
    let mut ann = TransactionAnnotation::new(transaction_id.clone());
    for patch in storage.get_transaction_annotation_patches(account_id).await? {
        if &patch.transaction_id == transaction_id {
            patch.apply_to(&mut ann);
        }
    }
    Ok(ann)
}

#[tokio::test]
async fn set_transaction_ignore_appends_ignore_patches_for_selected_transactions() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let storage = MemoryStorage::new();
    let config = resolved_config(dir.path());

    let account = Account::new("Checking", keepbook::models::Id::new());
    storage.save_account(&account).await?;

    let coffee = Transaction::new("-4.50", Asset::currency("USD"), "Coffee");
    let lunch = Transaction::new("-12.00", Asset::currency("USD"), "Lunch");
    storage
        .append_transactions(&account.id, &[coffee.clone(), lunch.clone()])
        .await?;

    let output = set_transaction_ignore(
        &storage,
        &config,
        vec![
            (account.id.to_string(), coffee.id.to_string()),
            (account.id.to_string(), lunch.id.to_string()),
        ],
        true,
    )
    .await?;

    assert_eq!(output["success"], true);
    assert_eq!(output["updated_count"], 2);
    assert_eq!(output["ignore"], true);

    for tx_id in [&coffee.id, &lunch.id] {
        let ann = materialized_annotation(&storage, &account.id, tx_id).await?;
        assert_eq!(ann.ignore_spending, Some(true));
        assert!(ann.ignores_spending());
    }

    Ok(())
}

#[tokio::test]
async fn set_transaction_ignore_false_clears_flag_and_strips_magic_tags() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let storage = MemoryStorage::new();
    let config = resolved_config(dir.path());

    let account = Account::new("Checking", keepbook::models::Id::new());
    storage.save_account(&account).await?;

    let wire = Transaction::new("-30000", Asset::currency("USD"), "Wire");
    storage
        .append_transactions(&account.id, &[wire.clone()])
        .await?;

    // Legacy-tagged transaction: magic tag plus a regular tag.
    keepbook::app::set_transaction_tags(
        &storage,
        &config,
        vec![(account.id.to_string(), wire.id.to_string())],
        vec!["ignore_spending".to_string(), "transfer".to_string()],
        false,
    )
    .await?;
    // Also explicitly ignored via the annotation flag.
    set_transaction_ignore(
        &storage,
        &config,
        vec![(account.id.to_string(), wire.id.to_string())],
        true,
    )
    .await?;

    let ann = materialized_annotation(&storage, &account.id, &wire.id).await?;
    assert!(ann.ignores_spending());

    let output = set_transaction_ignore(
        &storage,
        &config,
        vec![(account.id.to_string(), wire.id.to_string())],
        false,
    )
    .await?;
    assert_eq!(output["success"], true);
    assert_eq!(output["updated_count"], 1);
    assert_eq!(output["ignore"], false);

    let ann = materialized_annotation(&storage, &account.id, &wire.id).await?;
    assert_eq!(ann.ignore_spending, None);
    assert_eq!(ann.tags, Some(vec!["transfer".to_string()]));
    assert!(!ann.ignores_spending());

    Ok(())
}

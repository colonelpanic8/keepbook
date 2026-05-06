use std::path::Path;

use anyhow::Result;
use keepbook::app::set_transaction_categories;
use keepbook::config::{
    AiConfig, DisplayConfig, GitConfig, HistoryConfig, IgnoreConfig, PortfolioConfig,
    RefreshConfig, ResolvedConfig, SpendingConfig, TrayConfig,
};
use keepbook::models::{Account, Asset, Transaction};
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
        portfolio: PortfolioConfig::default(),
        ignore: IgnoreConfig::default(),
        ai: AiConfig::default(),
        git: GitConfig::default(),
    }
}

#[tokio::test]
async fn set_transaction_categories_appends_category_patches_for_selected_transactions(
) -> Result<()> {
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

    let output = set_transaction_categories(
        &storage,
        &config,
        vec![
            (account.id.to_string(), coffee.id.to_string()),
            (account.id.to_string(), lunch.id.to_string()),
        ],
        Some("Dining".to_string()),
        false,
    )
    .await?;

    assert_eq!(output["success"], true);
    assert_eq!(output["updated_count"], 2);
    assert_eq!(output["category"], "Dining");

    let patches = storage
        .get_transaction_annotation_patches(&account.id)
        .await?;
    assert_eq!(patches.len(), 2);
    assert!(patches.iter().all(|patch| matches!(
        patch.category.as_ref(),
        Some(Some(category)) if category == "Dining"
    )));

    Ok(())
}

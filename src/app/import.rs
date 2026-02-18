use std::path::Path;

use anyhow::{Context, Result};

use crate::config::ResolvedConfig;
use crate::storage::{find_account, Storage};
use crate::sync::schwab::parse_exported_transactions_json;

use super::maybe_auto_commit;

pub async fn import_schwab_transactions(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    account_id_or_name: &str,
    file: &Path,
) -> Result<serde_json::Value> {
    let account = find_account(storage, account_id_or_name)
        .await?
        .with_context(|| format!("Account not found: {account_id_or_name}"))?;

    let contents = std::fs::read_to_string(file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    let parsed = parse_exported_transactions_json(&account.id, &contents)
        .context("Failed to parse Schwab exported transactions JSON")?;

    if !parsed.transactions.is_empty() {
        storage
            .append_transactions(&account.id, &parsed.transactions)
            .await?;
        maybe_auto_commit(
            config,
            &format!(
                "import schwab transactions (account {})",
                account.id.as_str()
            ),
        );
    }

    Ok(serde_json::json!({
        "success": true,
        "account_id": account.id.to_string(),
        "imported": parsed.transactions.len(),
        "skipped": parsed.skipped,
    }))
}

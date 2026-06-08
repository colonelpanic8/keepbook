use std::path::Path;

use anyhow::{Context, Result};

use crate::config::ResolvedConfig;
use crate::storage::{find_account, Storage};
use crate::sync::schwab::parse_exported_transactions_json;

use super::{
    apply_transaction_rules_without_auto_commit, maybe_auto_commit, ApplyTransactionRulesOptions,
};

fn transaction_rules_apply_summary(result: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "success": result.get("success").cloned().unwrap_or(serde_json::Value::Bool(false)),
        "path": result.get("path").cloned().unwrap_or(serde_json::Value::Null),
        "rule_count": result.get("rule_count").cloned().unwrap_or(serde_json::Value::from(0)),
        "invalid_rule_count": result.get("invalid_rule_count").cloned().unwrap_or(serde_json::Value::from(0)),
        "matched_count": result.get("matched_count").cloned().unwrap_or(serde_json::Value::from(0)),
        "updated_count": result.get("updated_count").cloned().unwrap_or(serde_json::Value::from(0)),
        "skipped_existing_action_count": result
            .get("skipped_existing_action_count")
            .cloned()
            .unwrap_or(serde_json::Value::from(0)),
    })
}

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

    let transaction_rules = if parsed.transactions.is_empty() {
        None
    } else {
        storage
            .append_transactions(&account.id, &parsed.transactions)
            .await?;
        Some(
            apply_transaction_rules_without_auto_commit(
                storage,
                config,
                ApplyTransactionRulesOptions {
                    start: None,
                    end: None,
                    account: Some(account.id.to_string()),
                    connection: None,
                    overwrite: false,
                    dry_run: false,
                },
            )
            .await?,
        )
    };

    if !parsed.transactions.is_empty() {
        maybe_auto_commit(
            config,
            &format!(
                "import schwab transactions (account {})",
                account.id.as_str()
            ),
        );
    }

    let mut output = serde_json::json!({
        "success": true,
        "account_id": account.id.to_string(),
        "imported": parsed.transactions.len(),
        "skipped": parsed.skipped,
    });
    if let Some(transaction_rules) = transaction_rules {
        output["transaction_rules"] = transaction_rules_apply_summary(&transaction_rules);
    }
    Ok(output)
}

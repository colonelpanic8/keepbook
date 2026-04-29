use std::collections::HashSet;
use std::str::FromStr;

use anyhow::{Context, Result};

use crate::clock::{Clock, SystemClock};
use crate::config::ResolvedConfig;
use crate::models::{
    Account, AccountConfig, Asset, AssetBalance, BalanceBackfillPolicy, BalanceSnapshot,
    Connection, ConnectionConfig, ConnectionState, Id, IdGenerator, ProposedTransactionEdit,
    ProposedTransactionEditStatus, TransactionAnnotation, TransactionAnnotationPatch,
    UuidIdGenerator,
};
use crate::storage::{find_account, Storage};

use super::{maybe_auto_commit, ProposedTransactionEditOutput, TransactionAnnotationPatchOutput};

pub async fn remove_connection(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    id_str: &str,
) -> Result<serde_json::Value> {
    let id = Id::from_string_checked(id_str)
        .with_context(|| format!("Invalid connection id: {id_str}"))?;

    // Get connection info first
    let connection = storage.get_connection(&id).await?;
    let conn = match connection {
        Some(c) => c,
        None => {
            return Ok(serde_json::json!({
                "success": false,
                "error": "Connection not found",
                "id": id_str
            }));
        }
    };

    let name = conn.config.name.clone();
    let accounts = storage.list_accounts().await?;
    let valid_ids: HashSet<Id> = accounts
        .iter()
        .filter(|account| account.connection_id == *conn.id())
        .map(|account| account.id.clone())
        .collect();

    let mut account_ids: Vec<Id> = Vec::new();
    let mut seen_ids: HashSet<Id> = HashSet::new();
    for id in &conn.state.account_ids {
        if valid_ids.contains(id) && seen_ids.insert(id.clone()) {
            account_ids.push(id.clone());
        }
    }

    // Also include any accounts still linked to this connection ID (handles stale state).
    for account in accounts {
        if account.connection_id == *conn.id() && seen_ids.insert(account.id.clone()) {
            account_ids.push(account.id);
        }
    }

    // Delete all accounts belonging to this connection
    let mut deleted_accounts = 0;
    for account_id in &account_ids {
        if storage.delete_account(account_id).await? {
            deleted_accounts += 1;
        }
    }

    // Delete the connection
    storage.delete_connection(&id).await?;

    let result = serde_json::json!({
        "success": true,
        "connection": {
            "id": id_str,
            "name": name
        },
        "deleted_accounts": deleted_accounts,
        "account_ids": account_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>()
    });

    maybe_auto_commit(config, &format!("remove connection {id_str}"));

    Ok(result)
}

pub async fn add_connection(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    name: &str,
    synchronizer: &str,
) -> Result<serde_json::Value> {
    add_connection_with(
        storage,
        config,
        name,
        synchronizer,
        &UuidIdGenerator,
        &SystemClock,
    )
    .await
}

pub async fn add_connection_with(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    name: &str,
    synchronizer: &str,
    ids: &dyn IdGenerator,
    clock: &dyn Clock,
) -> Result<serde_json::Value> {
    let existing = storage
        .list_connections()
        .await?
        .into_iter()
        .find(|conn| conn.config.name.eq_ignore_ascii_case(name));
    if existing.is_some() {
        anyhow::bail!("Connection name already exists: {name}");
    }

    let connection = Connection {
        config: ConnectionConfig {
            name: name.to_string(),
            synchronizer: synchronizer.to_string(),
            credentials: None,
            balance_staleness: None,
        },
        state: ConnectionState::new_with_generator(ids, clock),
    };

    let id = connection.state.id.to_string();

    // Write the config TOML since save_connection only writes state.
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;

    // Save the connection (this creates the directory structure and symlinks)
    storage.save_connection(&connection).await?;

    let result = serde_json::json!({
        "success": true,
        "connection": {
            "id": id,
            "name": name,
            "synchronizer": synchronizer
        }
    });

    maybe_auto_commit(config, &format!("add connection {name}"));

    Ok(result)
}

pub async fn add_account(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    connection_id: &str,
    name: &str,
    tags: Vec<String>,
) -> Result<serde_json::Value> {
    add_account_with(
        storage,
        config,
        connection_id,
        name,
        tags,
        &UuidIdGenerator,
        &SystemClock,
    )
    .await
}

pub async fn add_account_with(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    connection_id: &str,
    name: &str,
    tags: Vec<String>,
    ids: &dyn IdGenerator,
    clock: &dyn Clock,
) -> Result<serde_json::Value> {
    let conn_id = Id::from_string_checked(connection_id)
        .with_context(|| format!("Invalid connection id: {connection_id}"))?;

    // Verify connection exists
    let mut connection = storage
        .get_connection(&conn_id)
        .await?
        .context("Connection not found")?;

    // Create account
    let mut account = Account::new_with_generator(ids, clock, name, conn_id.clone());
    account.tags = tags;

    let account_id = account.id.to_string();

    // Save account
    storage.save_account(&account).await?;

    // Update connection's account_ids
    connection.state.account_ids.push(account.id);
    storage.save_connection(&connection).await?;

    let result = serde_json::json!({
        "success": true,
        "account": {
            "id": account_id,
            "name": name,
            "connection_id": connection_id
        }
    });

    maybe_auto_commit(config, &format!("add account {name}"));

    Ok(result)
}

pub async fn set_balance(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    account_id: &str,
    asset_str: &str,
    amount: &str,
    cost_basis: Option<&str>,
) -> Result<serde_json::Value> {
    let amount = amount.trim();
    if amount.is_empty() {
        anyhow::bail!("Amount cannot be empty");
    }
    rust_decimal::Decimal::from_str(amount).with_context(|| format!("Invalid amount: {amount}"))?;
    let cost_basis = cost_basis.map(str::trim).filter(|value| !value.is_empty());
    if let Some(cost_basis) = cost_basis {
        rust_decimal::Decimal::from_str(cost_basis)
            .with_context(|| format!("Invalid cost basis: {cost_basis}"))?;
    }

    let id = Id::from_string_checked(account_id)
        .with_context(|| format!("Invalid account id: {account_id}"))?;

    // Verify account exists
    storage
        .get_account(&id)
        .await?
        .context("Account not found")?;

    // Parse asset string (formats: "USD", "equity:AAPL", "crypto:BTC")
    let asset = parse_asset(asset_str)?;

    // Create balance snapshot with single asset
    let mut asset_balance = AssetBalance::new(asset.clone(), amount);
    if let Some(cost_basis) = cost_basis {
        asset_balance = asset_balance.with_cost_basis(cost_basis);
    }
    let snapshot = BalanceSnapshot::now(vec![asset_balance]);

    // Append balance snapshot
    storage.append_balance_snapshot(&id, &snapshot).await?;

    let mut balance = serde_json::json!({
        "account_id": account_id,
        "asset": serde_json::to_value(&asset)?,
        "amount": amount,
        "timestamp": snapshot.timestamp.to_rfc3339()
    });
    if let Some(cost_basis) = cost_basis {
        balance["cost_basis"] = serde_json::json!(cost_basis);
    }

    let result = serde_json::json!({
        "success": true,
        "balance": balance
    });

    maybe_auto_commit(config, &format!("set balance {account_id} {asset_str}"));

    Ok(result)
}

fn parse_balance_backfill_policy(value: &str) -> Result<BalanceBackfillPolicy> {
    match value.trim().to_lowercase().as_str() {
        "none" => Ok(BalanceBackfillPolicy::None),
        "zero" => Ok(BalanceBackfillPolicy::Zero),
        "carry_earliest" | "carry-earliest" => Ok(BalanceBackfillPolicy::CarryEarliest),
        _ => anyhow::bail!(
            "Invalid balance backfill policy: {value}. Use: none, zero, carry_earliest"
        ),
    }
}

pub async fn set_account_config(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    account_id_or_name: &str,
    balance_backfill: Option<&str>,
    clear_balance_backfill: bool,
) -> Result<serde_json::Value> {
    if clear_balance_backfill && balance_backfill.is_some() {
        anyhow::bail!("Cannot use --balance-backfill and --clear-balance-backfill together");
    }

    if !clear_balance_backfill && balance_backfill.is_none() {
        anyhow::bail!("No account config fields specified");
    }

    let account = find_account(storage, account_id_or_name)
        .await?
        .context(format!("Account not found: {account_id_or_name}"))?;

    let mut account_config: AccountConfig =
        storage.get_account_config(&account.id)?.unwrap_or_default();
    if clear_balance_backfill {
        account_config.balance_backfill = None;
    } else if let Some(policy) = balance_backfill {
        account_config.balance_backfill = Some(parse_balance_backfill_policy(policy)?);
    }

    storage
        .save_account_config(&account.id, &account_config)
        .await?;

    let balance_backfill_json = match account_config.balance_backfill {
        Some(policy) => serde_json::to_value(policy)?,
        None => serde_json::Value::Null,
    };

    let result = serde_json::json!({
        "success": true,
        "account": {
            "id": account.id.to_string(),
            "name": account.name,
        },
        "config": {
            "balance_backfill": balance_backfill_json
        }
    });

    maybe_auto_commit(config, &format!("set account config {}", account.id));

    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn set_transaction_annotation(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    account_id: &str,
    transaction_id: &str,
    description: Option<String>,
    clear_description: bool,
    note: Option<String>,
    clear_note: bool,
    category: Option<String>,
    clear_category: bool,
    tags: Vec<String>,
    tags_empty: bool,
    clear_tags: bool,
    effective_date: Option<String>,
    clear_effective_date: bool,
) -> Result<serde_json::Value> {
    if clear_description && description.is_some() {
        anyhow::bail!("Cannot use --description and --clear-description together");
    }
    if clear_note && note.is_some() {
        anyhow::bail!("Cannot use --note and --clear-note together");
    }
    if clear_category && category.is_some() {
        anyhow::bail!("Cannot use --category and --clear-category together");
    }
    if clear_tags && (tags_empty || !tags.is_empty()) {
        anyhow::bail!("Cannot use --clear-tags with --tag/--tags-empty");
    }
    if clear_effective_date && effective_date.is_some() {
        anyhow::bail!("Cannot use --effective-date and --clear-effective-date together");
    }

    let acct_id = Id::from_string_checked(account_id)
        .with_context(|| format!("Invalid account id: {account_id}"))?;
    let tx_id = Id::from_string_checked(transaction_id)
        .with_context(|| format!("Invalid transaction id: {transaction_id}"))?;

    let has_change = description.is_some()
        || clear_description
        || note.is_some()
        || clear_note
        || category.is_some()
        || clear_category
        || !tags.is_empty()
        || tags_empty
        || clear_tags
        || effective_date.is_some()
        || clear_effective_date;
    if !has_change {
        anyhow::bail!("No annotation fields specified");
    }

    let parsed_effective_date = effective_date
        .as_deref()
        .map(|s| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .with_context(|| format!("Invalid effective date: {s}"))
        })
        .transpose()?;

    // Verify account exists.
    storage
        .get_account(&acct_id)
        .await?
        .context("Account not found")?;

    // Verify transaction exists for this account (annotation scope is per-account).
    let txns = storage.get_transactions(&acct_id).await?;
    if !txns.iter().any(|t| t.id == tx_id) {
        anyhow::bail!("Transaction not found for account");
    }

    let mut patch = TransactionAnnotationPatch {
        transaction_id: tx_id.clone(),
        timestamp: chrono::Utc::now(),
        description: None,
        note: None,
        category: None,
        tags: None,
        effective_date: None,
    };

    if clear_description {
        patch.description = Some(None);
    } else if let Some(v) = description {
        patch.description = Some(Some(v));
    }
    if clear_note {
        patch.note = Some(None);
    } else if let Some(v) = note {
        patch.note = Some(Some(v));
    }
    if clear_category {
        patch.category = Some(None);
    } else if let Some(v) = category {
        patch.category = Some(Some(v));
    }
    if clear_tags {
        patch.tags = Some(None);
    } else if tags_empty {
        patch.tags = Some(Some(Vec::new()));
    } else if !tags.is_empty() {
        patch.tags = Some(Some(tags));
    }
    if clear_effective_date {
        patch.effective_date = Some(None);
    } else if let Some(v) = parsed_effective_date {
        patch.effective_date = Some(Some(v));
    }

    storage
        .append_transaction_annotation_patches(&acct_id, &[patch.clone()])
        .await?;

    // Materialize current annotation state for the transaction.
    let patches = storage.get_transaction_annotation_patches(&acct_id).await?;
    let mut ann = TransactionAnnotation::new(tx_id.clone());
    for p in patches.into_iter().filter(|p| p.transaction_id == tx_id) {
        p.apply_to(&mut ann);
    }

    let mut patch_json = serde_json::Map::new();
    patch_json.insert(
        "timestamp".to_string(),
        serde_json::json!(patch.timestamp.to_rfc3339()),
    );
    if let Some(v) = patch.description {
        patch_json.insert(
            "description".to_string(),
            match v {
                Some(s) => serde_json::json!(s),
                None => serde_json::Value::Null,
            },
        );
    }
    if let Some(v) = patch.note {
        patch_json.insert(
            "note".to_string(),
            match v {
                Some(s) => serde_json::json!(s),
                None => serde_json::Value::Null,
            },
        );
    }
    if let Some(v) = patch.category {
        patch_json.insert(
            "category".to_string(),
            match v {
                Some(s) => serde_json::json!(s),
                None => serde_json::Value::Null,
            },
        );
    }
    if let Some(v) = patch.tags {
        patch_json.insert(
            "tags".to_string(),
            match v {
                Some(tags) => serde_json::json!(tags),
                None => serde_json::Value::Null,
            },
        );
    }
    if let Some(v) = patch.effective_date {
        patch_json.insert(
            "effective_date".to_string(),
            match v {
                Some(date) => serde_json::json!(date.to_string()),
                None => serde_json::Value::Null,
            },
        );
    }

    let annotation_json = if ann.is_empty() {
        serde_json::Value::Null
    } else {
        let mut m = serde_json::Map::new();
        if let Some(v) = ann.description {
            m.insert("description".to_string(), serde_json::json!(v));
        }
        if let Some(v) = ann.note {
            m.insert("note".to_string(), serde_json::json!(v));
        }
        if let Some(v) = ann.category {
            m.insert("category".to_string(), serde_json::json!(v));
        }
        if let Some(v) = ann.tags {
            m.insert("tags".to_string(), serde_json::json!(v));
        }
        if let Some(v) = ann.effective_date {
            m.insert(
                "effective_date".to_string(),
                serde_json::json!(v.to_string()),
            );
        }
        serde_json::Value::Object(m)
    };

    let result = serde_json::json!({
        "success": true,
        "account_id": account_id,
        "transaction_id": transaction_id,
        "patch": serde_json::Value::Object(patch_json),
        "annotation": annotation_json
    });

    maybe_auto_commit(
        config,
        &format!("set transaction annotation {account_id} {transaction_id}"),
    );

    Ok(result)
}

#[allow(clippy::too_many_arguments)]
pub async fn propose_transaction_edit(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    account_id: &str,
    transaction_id: &str,
    description: Option<String>,
    clear_description: bool,
    note: Option<String>,
    clear_note: bool,
    category: Option<String>,
    clear_category: bool,
    tags: Vec<String>,
    tags_empty: bool,
    clear_tags: bool,
    effective_date: Option<String>,
    clear_effective_date: bool,
) -> Result<serde_json::Value> {
    propose_transaction_edit_with(
        storage,
        config,
        account_id,
        transaction_id,
        description,
        clear_description,
        note,
        clear_note,
        category,
        clear_category,
        tags,
        tags_empty,
        clear_tags,
        effective_date,
        clear_effective_date,
        &UuidIdGenerator,
        &SystemClock,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn propose_transaction_edit_with(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    account_id: &str,
    transaction_id: &str,
    description: Option<String>,
    clear_description: bool,
    note: Option<String>,
    clear_note: bool,
    category: Option<String>,
    clear_category: bool,
    tags: Vec<String>,
    tags_empty: bool,
    clear_tags: bool,
    effective_date: Option<String>,
    clear_effective_date: bool,
    ids: &dyn IdGenerator,
    clock: &dyn Clock,
) -> Result<serde_json::Value> {
    let patch = build_transaction_annotation_patch(
        description,
        clear_description,
        note,
        clear_note,
        category,
        clear_category,
        tags,
        tags_empty,
        clear_tags,
        effective_date,
        clear_effective_date,
    )?;

    let acct_id = Id::from_string_checked(account_id)
        .with_context(|| format!("Invalid account id: {account_id}"))?;
    let tx_id = Id::from_string_checked(transaction_id)
        .with_context(|| format!("Invalid transaction id: {transaction_id}"))?;

    storage
        .get_account(&acct_id)
        .await?
        .context("Account not found")?;
    let txns = storage.get_transactions(&acct_id).await?;
    if !txns.iter().any(|t| t.id == tx_id) {
        anyhow::bail!("Transaction not found for account");
    }

    let now = clock.now();
    let edit = ProposedTransactionEdit {
        id: ids.new_id(),
        account_id: acct_id,
        transaction_id: tx_id,
        created_at: now,
        updated_at: now,
        status: ProposedTransactionEditStatus::Pending,
        description: patch.description,
        note: patch.note,
        category: patch.category,
        tags: patch.tags,
        effective_date: patch.effective_date,
    };
    storage
        .append_proposed_transaction_edits(std::slice::from_ref(&edit))
        .await?;

    let result = serde_json::json!({
        "success": true,
        "proposal": proposal_to_json(&edit)?
    });

    maybe_auto_commit(config, &format!("propose transaction edit {}", edit.id));
    Ok(result)
}

pub async fn list_proposed_transaction_edits(
    storage: &dyn Storage,
    include_decided: bool,
) -> Result<Vec<ProposedTransactionEditOutput>> {
    let accounts = storage.list_accounts().await?;
    let accounts_by_id: std::collections::HashMap<Id, Account> = accounts
        .into_iter()
        .map(|account| (account.id.clone(), account))
        .collect();
    let mut output = Vec::new();

    for edit in storage.get_proposed_transaction_edits().await? {
        if !include_decided && edit.status != ProposedTransactionEditStatus::Pending {
            continue;
        }
        if let Some(rendered) =
            render_proposed_transaction_edit(storage, &accounts_by_id, &edit).await?
        {
            output.push(rendered);
        }
    }

    Ok(output)
}

pub async fn approve_proposed_transaction_edit(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    proposal_id: &str,
) -> Result<serde_json::Value> {
    decide_proposed_transaction_edit(
        storage,
        config,
        proposal_id,
        ProposedTransactionEditStatus::Approved,
    )
    .await
}

pub async fn reject_proposed_transaction_edit(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    proposal_id: &str,
) -> Result<serde_json::Value> {
    decide_proposed_transaction_edit(
        storage,
        config,
        proposal_id,
        ProposedTransactionEditStatus::Rejected,
    )
    .await
}

pub async fn remove_proposed_transaction_edit(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    proposal_id: &str,
) -> Result<serde_json::Value> {
    decide_proposed_transaction_edit(
        storage,
        config,
        proposal_id,
        ProposedTransactionEditStatus::Removed,
    )
    .await
}

async fn decide_proposed_transaction_edit(
    storage: &dyn Storage,
    config: &ResolvedConfig,
    proposal_id: &str,
    status: ProposedTransactionEditStatus,
) -> Result<serde_json::Value> {
    let id = Id::from_string_checked(proposal_id)
        .with_context(|| format!("Invalid proposal id: {proposal_id}"))?;
    let edit = storage
        .get_proposed_transaction_edits()
        .await?
        .into_iter()
        .find(|edit| edit.id == id)
        .context("Proposed transaction edit not found")?;

    if edit.status != ProposedTransactionEditStatus::Pending {
        anyhow::bail!("Proposed transaction edit is already decided");
    }

    let now = chrono::Utc::now();
    if status == ProposedTransactionEditStatus::Approved {
        let patch = edit.to_annotation_patch(now);
        storage
            .append_transaction_annotation_patches(&edit.account_id, &[patch])
            .await?;
    }

    let decision = edit.with_status(status, now);
    storage
        .append_proposed_transaction_edits(std::slice::from_ref(&decision))
        .await?;

    let result = serde_json::json!({
        "success": true,
        "proposal": proposal_to_json(&decision)?
    });
    maybe_auto_commit(
        config,
        &format!("decide proposed transaction edit {proposal_id}"),
    );
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn build_transaction_annotation_patch(
    description: Option<String>,
    clear_description: bool,
    note: Option<String>,
    clear_note: bool,
    category: Option<String>,
    clear_category: bool,
    tags: Vec<String>,
    tags_empty: bool,
    clear_tags: bool,
    effective_date: Option<String>,
    clear_effective_date: bool,
) -> Result<TransactionAnnotationPatch> {
    if clear_description && description.is_some() {
        anyhow::bail!("Cannot use --description and --clear-description together");
    }
    if clear_note && note.is_some() {
        anyhow::bail!("Cannot use --note and --clear-note together");
    }
    if clear_category && category.is_some() {
        anyhow::bail!("Cannot use --category and --clear-category together");
    }
    if clear_tags && (tags_empty || !tags.is_empty()) {
        anyhow::bail!("Cannot use --clear-tags with --tag/--tags-empty");
    }
    if clear_effective_date && effective_date.is_some() {
        anyhow::bail!("Cannot use --effective-date and --clear-effective-date together");
    }

    let parsed_effective_date = effective_date
        .as_deref()
        .map(|s| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .with_context(|| format!("Invalid effective date: {s}"))
        })
        .transpose()?;

    let has_change = description.is_some()
        || clear_description
        || note.is_some()
        || clear_note
        || category.is_some()
        || clear_category
        || !tags.is_empty()
        || tags_empty
        || clear_tags
        || effective_date.is_some()
        || clear_effective_date;
    if !has_change {
        anyhow::bail!("No annotation fields specified");
    }

    let mut patch = TransactionAnnotationPatch {
        transaction_id: Id::from("pending"),
        timestamp: chrono::Utc::now(),
        description: None,
        note: None,
        category: None,
        tags: None,
        effective_date: None,
    };
    if clear_description {
        patch.description = Some(None);
    } else if let Some(v) = description {
        patch.description = Some(Some(v));
    }
    if clear_note {
        patch.note = Some(None);
    } else if let Some(v) = note {
        patch.note = Some(Some(v));
    }
    if clear_category {
        patch.category = Some(None);
    } else if let Some(v) = category {
        patch.category = Some(Some(v));
    }
    if clear_tags {
        patch.tags = Some(None);
    } else if tags_empty {
        patch.tags = Some(Some(Vec::new()));
    } else if !tags.is_empty() {
        patch.tags = Some(Some(tags));
    }
    if clear_effective_date {
        patch.effective_date = Some(None);
    } else if let Some(v) = parsed_effective_date {
        patch.effective_date = Some(Some(v));
    }

    Ok(patch)
}

fn proposal_to_json(edit: &ProposedTransactionEdit) -> Result<serde_json::Value> {
    Ok(serde_json::json!({
        "id": edit.id.to_string(),
        "account_id": edit.account_id.to_string(),
        "transaction_id": edit.transaction_id.to_string(),
        "created_at": edit.created_at.to_rfc3339(),
        "updated_at": edit.updated_at.to_rfc3339(),
        "status": proposal_status_string(edit.status),
        "patch": serde_json::to_value(proposal_patch_output(edit))?
    }))
}

async fn render_proposed_transaction_edit(
    storage: &dyn Storage,
    accounts_by_id: &std::collections::HashMap<Id, Account>,
    edit: &ProposedTransactionEdit,
) -> Result<Option<ProposedTransactionEditOutput>> {
    let Some(account) = accounts_by_id.get(&edit.account_id) else {
        return Ok(None);
    };
    let transaction = storage
        .get_transactions(&edit.account_id)
        .await?
        .into_iter()
        .find(|tx| tx.id == edit.transaction_id);
    let Some(transaction) = transaction else {
        return Ok(None);
    };

    Ok(Some(ProposedTransactionEditOutput {
        id: edit.id.to_string(),
        account_id: edit.account_id.to_string(),
        account_name: account.name.clone(),
        transaction_id: edit.transaction_id.to_string(),
        transaction_description: transaction.description,
        transaction_timestamp: transaction.timestamp.to_rfc3339(),
        transaction_amount: transaction.amount,
        created_at: edit.created_at.to_rfc3339(),
        updated_at: edit.updated_at.to_rfc3339(),
        status: proposal_status_string(edit.status).to_string(),
        patch: proposal_patch_output(edit),
    }))
}

fn proposal_patch_output(edit: &ProposedTransactionEdit) -> TransactionAnnotationPatchOutput {
    TransactionAnnotationPatchOutput {
        description: edit.description.clone(),
        note: edit.note.clone(),
        category: edit.category.clone(),
        tags: edit.tags.clone(),
        effective_date: edit
            .effective_date
            .map(|value| value.map(|date| date.to_string())),
    }
}

fn proposal_status_string(status: ProposedTransactionEditStatus) -> &'static str {
    match status {
        ProposedTransactionEditStatus::Pending => "pending",
        ProposedTransactionEditStatus::Approved => "approved",
        ProposedTransactionEditStatus::Rejected => "rejected",
        ProposedTransactionEditStatus::Removed => "removed",
    }
}

pub fn parse_asset(s: &str) -> Result<Asset> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Asset string cannot be empty");
    }
    if let Some((prefix, value)) = trimmed.split_once(':') {
        let value = value.trim();
        if value.is_empty() {
            anyhow::bail!("Asset value missing for prefix '{prefix}'");
        }
        match prefix.to_lowercase().as_str() {
            "equity" => return Ok(Asset::equity(value)),
            "crypto" => return Ok(Asset::crypto(value)),
            "currency" => return Ok(Asset::currency(value)),
            _ => {}
        }
    }

    // Assume it's a currency code
    Ok(Asset::currency(trimmed))
}

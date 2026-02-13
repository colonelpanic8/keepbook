use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use keepbook::models::{
    Account, Asset, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig, ConnectionState,
    ConnectionStatus, Id,
};
use keepbook::storage::{JsonFileStorage, Storage};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

#[derive(Debug, Clone, Deserialize)]
struct ContractCase {
    name: String,
    reporting_currency: Option<String>,
    #[serde(default)]
    seed: Seed,
    command: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Seed {
    #[serde(default)]
    connections: Vec<SeedConnection>,
    #[serde(default)]
    accounts: Vec<SeedAccount>,
    #[serde(default)]
    balance_snapshots: Vec<SeedBalanceSnapshot>,
}

#[derive(Debug, Clone, Deserialize)]
struct SeedConnection {
    id: String,
    name: String,
    #[serde(default = "default_synchronizer")]
    synchronizer: String,
    #[serde(default = "default_connection_status")]
    status: String,
    #[serde(default)]
    account_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SeedAccount {
    id: String,
    name: String,
    connection_id: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default = "default_true")]
    active: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct SeedBalanceSnapshot {
    account_id: String,
    timestamp: String,
    balances: Vec<SeedAssetBalance>,
}

#[derive(Debug, Clone, Deserialize)]
struct SeedAssetBalance {
    asset: serde_json::Value,
    amount: String,
}

fn default_true() -> bool {
    true
}

fn default_synchronizer() -> String {
    "manual".to_string()
}

fn default_connection_status() -> String {
    "active".to_string()
}

fn fixed_created_at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()
}

fn parse_conn_status(value: &str) -> Result<ConnectionStatus> {
    match value {
        "active" => Ok(ConnectionStatus::Active),
        "error" => Ok(ConnectionStatus::Error),
        "disconnected" => Ok(ConnectionStatus::Disconnected),
        "pending_reauth" => Ok(ConnectionStatus::PendingReauth),
        _ => anyhow::bail!("Invalid connection status: {value}"),
    }
}

async fn seed_storage(storage: &dyn Storage, seed: &Seed) -> Result<()> {
    for c in &seed.connections {
        let id = Id::from_string_checked(&c.id).context("Invalid connection id")?;
        let mut state = ConnectionState::new_with(id.clone(), fixed_created_at());
        state.status = parse_conn_status(&c.status)?;
        let mut account_ids = Vec::new();
        for s in &c.account_ids {
            account_ids.push(
                Id::from_string_checked(s)
                    .with_context(|| format!("Invalid account id in connection '{}': {}", c.id, s))?,
            );
        }
        state.account_ids = account_ids;

        let conn = Connection {
            config: ConnectionConfig {
                name: c.name.clone(),
                synchronizer: c.synchronizer.clone(),
                credentials: None,
                balance_staleness: None,
            },
            state,
        };

        storage
            .save_connection_config(conn.id(), &conn.config)
            .await?;
        storage.save_connection(&conn).await?;
    }

    for a in &seed.accounts {
        let id = Id::from_string_checked(&a.id).context("Invalid account id")?;
        let conn_id = Id::from_string_checked(&a.connection_id).context("Invalid connection id")?;
        let mut acct = Account::new_with(id, fixed_created_at(), a.name.clone(), conn_id);
        acct.tags = a.tags.clone();
        acct.active = a.active;
        storage.save_account(&acct).await?;
    }

    for s in &seed.balance_snapshots {
        let account_id = Id::from_string_checked(&s.account_id).context("Invalid account id")?;
        let timestamp: DateTime<Utc> = s
            .timestamp
            .parse()
            .with_context(|| format!("Invalid snapshot timestamp: {}", s.timestamp))?;

        let mut balances = Vec::new();
        for b in &s.balances {
            let asset: Asset =
                serde_json::from_value(b.asset.clone()).context("Invalid asset JSON")?;
            balances.push(AssetBalance::new(asset, b.amount.clone()));
        }
        let snapshot = BalanceSnapshot::new(timestamp, balances);
        storage.append_balance_snapshot(&account_id, &snapshot).await?;
    }

    Ok(())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn contracts_dir() -> PathBuf {
    repo_root().join("contracts")
}

fn load_cases() -> Result<Vec<ContractCase>> {
    let path = contracts_dir().join("cases.json");
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&content).context("Failed to parse contracts/cases.json")
}

fn write_config(config_path: &Path, data_dir: &Path, reporting_currency: &str) -> Result<()> {
    let content = format!(
        r#"
data_dir = "{}"
reporting_currency = "{}"

[refresh]
balance_staleness = "14d"
price_staleness = "24h"

[git]
auto_commit = false
auto_push = false
merge_master_before_command = false
"#,
        data_dir.display(),
        reporting_currency
    );
    std::fs::write(config_path, content).context("Failed to write config")
}

fn run_rust_cli(config_path: &Path, args: &[String]) -> Result<serde_json::Value> {
    let output = Command::new(env!("CARGO_BIN_EXE_keepbook"))
        .args(["--config", config_path.to_str().unwrap()])
        .args(args)
        .output()
        .context("Failed to execute Rust CLI")?;

    if !output.status.success() {
        anyhow::bail!("Rust CLI failed: {output:?}");
    }

    let stdout = String::from_utf8(output.stdout).context("Rust CLI stdout not UTF-8")?;
    serde_json::from_str(&stdout).context("Rust CLI output was not valid JSON")
}

fn run_ts_cli(config_path: &Path, args: &[String]) -> Result<serde_json::Value> {
    let entry = repo_root().join("ts").join("dist").join("cli").join("main.js");
    if !entry.exists() {
        anyhow::bail!("Missing TS CLI entrypoint: {}", entry.display());
    }

    let output = Command::new("node")
        .arg(entry)
        .args(["--config", config_path.to_str().unwrap()])
        .args(args)
        .output()
        .context("Failed to execute TS CLI (node)")?;

    if !output.status.success() {
        anyhow::bail!("TS CLI failed: {output:?}");
    }

    let stdout = String::from_utf8(output.stdout).context("TS CLI stdout not UTF-8")?;
    serde_json::from_str(&stdout).context("TS CLI output was not valid JSON")
}

fn load_expected(name: &str) -> Result<serde_json::Value> {
    let path = contracts_dir().join(format!("{name}.json"));
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("Invalid JSON fixture: {name}.json"))
}

#[test]
fn contracts_match_both_clis() -> Result<()> {
    let cases = load_cases()?;

    let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;

    for case in cases {
        let temp = TempDir::new().context("Failed to create temp dir")?;
        let config_path = temp.path().join("keepbook.toml");

        let reporting_currency = case.reporting_currency.clone().unwrap_or("USD".to_string());
        write_config(&config_path, temp.path(), &reporting_currency)?;

        let storage = JsonFileStorage::new(temp.path());
        rt.block_on(async { seed_storage(&storage, &case.seed).await })?;

        let rust_json = run_rust_cli(&config_path, &case.command)
            .with_context(|| format!("Case '{}' (Rust CLI)", case.name))?;
        let ts_json = run_ts_cli(&config_path, &case.command)
            .with_context(|| format!("Case '{}' (TS CLI)", case.name))?;
        let expected = load_expected(&case.name)?;

        if rust_json != expected {
            anyhow::bail!(
                "Contract '{}' mismatch (Rust vs expected)\nRust: {}\nExpected: {}",
                case.name,
                serde_json::to_string_pretty(&rust_json).unwrap_or_default(),
                serde_json::to_string_pretty(&expected).unwrap_or_default()
            );
        }

        if ts_json != expected {
            anyhow::bail!(
                "Contract '{}' mismatch (TS vs expected)\nTS: {}\nExpected: {}",
                case.name,
                serde_json::to_string_pretty(&ts_json).unwrap_or_default(),
                serde_json::to_string_pretty(&expected).unwrap_or_default()
            );
        }

        if rust_json != ts_json {
            anyhow::bail!(
                "Contract '{}' mismatch (Rust vs TS)\nRust: {}\nTS: {}",
                case.name,
                serde_json::to_string_pretty(&rust_json).unwrap_or_default(),
                serde_json::to_string_pretty(&ts_json).unwrap_or_default()
            );
        }
    }

    Ok(())
}

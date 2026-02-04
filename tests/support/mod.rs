use std::path::Path;
use std::process::Command;

use anyhow::Result;
use async_trait::async_trait;
use keepbook::models::{
    Account, Asset, AssetBalance, Connection, ConnectionConfig, Transaction,
};
use keepbook::sync::{SyncResult, SyncedAssetBalance, Synchronizer};

pub fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn run_git(dir: &Path, args: &[&str]) -> Result<std::process::Output> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()?;
    Ok(output)
}

pub fn init_repo(dir: &Path) -> Result<()> {
    let init = run_git(dir, &["init"])?;
    if !init.status.success() {
        anyhow::bail!("git init failed");
    }
    let email = run_git(dir, &["config", "user.email", "test@example.com"])?;
    if !email.status.success() {
        anyhow::bail!("git config user.email failed");
    }
    let name = run_git(dir, &["config", "user.name", "Keepbook Test"])?;
    if !name.status.success() {
        anyhow::bail!("git config user.name failed");
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct MockSynchronizer {
    pub name: String,
    pub account_name: String,
    pub asset: Asset,
    pub balance_amount: String,
    pub transaction_amount: String,
    pub transaction_description: String,
}

impl Default for MockSynchronizer {
    fn default() -> Self {
        Self {
            name: "mock".to_string(),
            account_name: "Mock Checking".to_string(),
            asset: Asset::currency("USD"),
            balance_amount: "123.45".to_string(),
            transaction_amount: "-10.00".to_string(),
            transaction_description: "Test purchase".to_string(),
        }
    }
}

impl MockSynchronizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_account_name(mut self, name: impl Into<String>) -> Self {
        self.account_name = name.into();
        self
    }

    pub fn with_asset(mut self, asset: Asset) -> Self {
        self.asset = asset;
        self
    }

    pub fn with_balance_amount(mut self, amount: impl Into<String>) -> Self {
        self.balance_amount = amount.into();
        self
    }

    pub fn with_transaction(mut self, amount: impl Into<String>, description: impl Into<String>) -> Self {
        self.transaction_amount = amount.into();
        self.transaction_description = description.into();
        self
    }
}

#[async_trait]
impl Synchronizer for MockSynchronizer {
    fn name(&self) -> &str {
        &self.name
    }

    async fn sync(&self, connection: &mut Connection) -> Result<SyncResult> {
        let account = Account::new(self.account_name.clone(), connection.id().clone());
        connection.state.account_ids = vec![account.id.clone()];

        let balance = SyncedAssetBalance::new(AssetBalance::new(
            self.asset.clone(),
            self.balance_amount.clone(),
        ));
        let transaction = Transaction::new(
            self.transaction_amount.clone(),
            self.asset.clone(),
            self.transaction_description.clone(),
        );

        Ok(SyncResult {
            connection: connection.clone(),
            accounts: vec![account.clone()],
            balances: vec![(account.id.clone(), vec![balance])],
            transactions: vec![(account.id.clone(), vec![transaction])],
        })
    }
}

pub fn mock_connection(name: impl Into<String>) -> Connection {
    Connection::new(ConnectionConfig {
        name: name.into(),
        synchronizer: "mock".to_string(),
        credentials: None,
        balance_staleness: None,
    })
}

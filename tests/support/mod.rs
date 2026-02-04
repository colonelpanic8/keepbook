#![allow(dead_code)]

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use keepbook::market_data::{AssetId, FxRateKind, FxRatePoint, MarketDataSource, PriceKind, PricePoint};
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

#[derive(Debug, Clone, Default)]
pub struct MockMarketDataSource {
    prices: HashMap<(AssetId, NaiveDate), PricePoint>,
    fx_rates: HashMap<(String, String, NaiveDate), FxRatePoint>,
    fail_on_fetch: bool,
}

impl MockMarketDataSource {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_price(mut self, price: PricePoint) -> Self {
        self.prices
            .insert((price.asset_id.clone(), price.as_of_date), price);
        self
    }

    pub fn with_fx_rate(mut self, rate: FxRatePoint) -> Self {
        self.fx_rates
            .insert((rate.base.clone(), rate.quote.clone(), rate.as_of_date), rate);
        self
    }

    pub fn fail_on_fetch(mut self) -> Self {
        self.fail_on_fetch = true;
        self
    }
}

#[async_trait]
impl MarketDataSource for MockMarketDataSource {
    async fn fetch_price(
        &self,
        _asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        if self.fail_on_fetch {
            anyhow::bail!("mock price fetch called unexpectedly");
        }
        Ok(self.prices.get(&(asset_id.clone(), date)).cloned())
    }

    async fn fetch_fx_rate(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
    ) -> Result<Option<FxRatePoint>> {
        if self.fail_on_fetch {
            anyhow::bail!("mock fx fetch called unexpectedly");
        }
        Ok(self
            .fx_rates
            .get(&(base.to_string(), quote.to_string(), date))
            .cloned())
    }

    fn name(&self) -> &str {
        "mock"
    }
}

pub fn price_point(
    asset: &Asset,
    date: NaiveDate,
    price: impl Into<String>,
    quote_currency: impl Into<String>,
    kind: PriceKind,
) -> PricePoint {
    price_point_with_timestamp(
        asset,
        date,
        price,
        quote_currency,
        kind,
        Utc::now(),
    )
}

pub fn price_point_with_timestamp(
    asset: &Asset,
    date: NaiveDate,
    price: impl Into<String>,
    quote_currency: impl Into<String>,
    kind: PriceKind,
    timestamp: DateTime<Utc>,
) -> PricePoint {
    PricePoint {
        asset_id: AssetId::from_asset(asset),
        as_of_date: date,
        timestamp,
        price: price.into(),
        quote_currency: quote_currency.into(),
        kind,
        source: "mock".to_string(),
    }
}

pub fn fx_rate_point(
    base: impl Into<String>,
    quote: impl Into<String>,
    date: NaiveDate,
    rate: impl Into<String>,
) -> FxRatePoint {
    fx_rate_point_with_timestamp(base, quote, date, rate, Utc::now())
}

pub fn fx_rate_point_with_timestamp(
    base: impl Into<String>,
    quote: impl Into<String>,
    date: NaiveDate,
    rate: impl Into<String>,
    timestamp: DateTime<Utc>,
) -> FxRatePoint {
    FxRatePoint {
        base: base.into(),
        quote: quote.into(),
        as_of_date: date,
        timestamp,
        rate: rate.into(),
        kind: FxRateKind::Close,
        source: "mock".to_string(),
    }
}

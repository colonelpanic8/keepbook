use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{TimeZone, Utc};

use super::*;
use crate::clock::FixedClock;
use crate::market_data::{
    AssetId, EquityPriceRouter, EquityPriceSource, FxRatePoint, FxRateRouter, FxRateSource,
    MarketDataStore, MemoryMarketDataStore, PriceKind, PricePoint,
};
use crate::models::{Account, AssetBalance, BalanceSnapshot, ConnectionConfig, Transaction};
use crate::storage::MemoryStorage;
use crate::sync::{AccountBalances, AccountListing, SyncOptions, SyncedAssetBalance};

struct CountingEquitySource {
    closes: AtomicUsize,
    quotes: AtomicUsize,
    currency: &'static str,
}

impl CountingEquitySource {
    fn new(currency: &'static str) -> Self {
        Self {
            closes: AtomicUsize::new(0),
            quotes: AtomicUsize::new(0),
            currency,
        }
    }

    fn point(&self, asset_id: &AssetId, date: NaiveDate, kind: PriceKind) -> PricePoint {
        PricePoint {
            asset_id: asset_id.clone(),
            as_of_date: date,
            timestamp: Utc.with_ymd_and_hms(2026, 3, 2, 21, 0, 0).unwrap(),
            price: "100".to_string(),
            quote_currency: self.currency.to_string(),
            kind,
            source: "counting".to_string(),
        }
    }
}

#[async_trait::async_trait]
impl EquityPriceSource for CountingEquitySource {
    async fn fetch_close(
        &self,
        _asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(Some(self.point(asset_id, date, PriceKind::Close)))
    }

    async fn fetch_quote(&self, _asset: &Asset, asset_id: &AssetId) -> Result<Option<PricePoint>> {
        self.quotes.fetch_add(1, Ordering::SeqCst);
        let date = Utc
            .with_ymd_and_hms(2026, 3, 2, 21, 0, 0)
            .unwrap()
            .date_naive();
        Ok(Some(self.point(asset_id, date, PriceKind::Quote)))
    }

    fn name(&self) -> &str {
        "counting"
    }
}

struct CountingFxSource {
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl FxRateSource for CountingFxSource {
    async fn fetch_close(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
    ) -> Result<Option<FxRatePoint>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Some(FxRatePoint {
            base: base.to_string(),
            quote: quote.to_string(),
            as_of_date: date,
            timestamp: Utc.with_ymd_and_hms(2026, 3, 2, 21, 0, 0).unwrap(),
            rate: "1.1".to_string(),
            kind: crate::market_data::FxRateKind::Close,
            source: "counting".to_string(),
        }))
    }

    fn name(&self) -> &str {
        "counting"
    }
}

struct PricedSynchronizer {
    account: Account,
    balances: Vec<SyncedAssetBalance>,
    transactions: Vec<Transaction>,
}

#[async_trait::async_trait]
impl Synchronizer for PricedSynchronizer {
    fn name(&self) -> &str {
        "priced"
    }

    async fn sync(
        &self,
        connection: &mut Connection,
        _storage: &dyn Storage,
    ) -> Result<SyncResult> {
        connection.state.account_ids = vec![self.account.id.clone()];
        Ok(SyncResult {
            connection: connection.clone(),
            accounts: vec![self.account.clone()],
            account_listing: AccountListing::Complete,
            balances: vec![(
                self.account.id.clone(),
                AccountBalances::snapshot(self.balances.clone()),
            )],
            transactions: vec![(self.account.id.clone(), self.transactions.clone())],
        })
    }
}

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 3, 2).unwrap()
}

fn clock() -> Arc<FixedClock> {
    Arc::new(FixedClock::new(
        Utc.with_ymd_and_hms(2026, 3, 2, 12, 0, 0).unwrap(),
    ))
}

fn orchestrator(
    storage: Arc<MemoryStorage>,
    store: Arc<MemoryMarketDataStore>,
    equity: Option<Arc<EquityPriceRouter>>,
    fx: Option<Arc<FxRateRouter>>,
    reporting_currency: &str,
) -> SyncOrchestrator {
    let mut market_data = MarketDataService::new(store, None).with_clock(clock());
    if let Some(equity) = equity {
        market_data = market_data.with_equity_router(equity);
    }
    if let Some(fx) = fx {
        market_data = market_data.with_fx_router(fx);
    }
    SyncOrchestrator::new(
        storage as Arc<dyn Storage>,
        market_data,
        reporting_currency.to_string(),
    )
    .with_clock(clock())
}

fn connection() -> Connection {
    Connection::new(ConnectionConfig {
        name: "Priced".to_string(),
        synchronizer: "priced".to_string(),
        credentials: None,
        balance_staleness: None,
    })
}

#[tokio::test]
async fn sync_with_prices_persists_synchronizer_supplied_prices() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let store = Arc::new(MemoryMarketDataStore::new());
    let mut connection = connection();
    let account = Account::new_with(
        Id::from_string("acct-1"),
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        "Brokerage",
        connection.id().clone(),
    );
    let asset = Asset::equity("AAPL");
    let price = PricePoint {
        asset_id: AssetId::from_asset(&asset),
        as_of_date: today(),
        timestamp: Utc.with_ymd_and_hms(2026, 3, 2, 21, 0, 0).unwrap(),
        price: "212.50".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: "synchronizer".to_string(),
    };
    let synchronizer = PricedSynchronizer {
        account: account.clone(),
        balances: vec![
            SyncedAssetBalance::new(AssetBalance::new(asset.clone(), "3"))
                .with_price(price.clone()),
            SyncedAssetBalance::new(AssetBalance::new(Asset::currency("USD"), "100")),
        ],
        transactions: vec![
            Transaction::new("-12.00", Asset::currency("USD"), "Coffee Shop")
                .with_id(Id::from_string("tx-1")),
        ],
    };

    let orchestrator = orchestrator(storage.clone(), store.clone(), None, None, "USD");
    let report = orchestrator
        .sync_with_prices(
            &synchronizer,
            &mut connection,
            false,
            &SyncOptions::default(),
        )
        .await?;

    assert_eq!(report.stored_prices, 1);
    let stored = store
        .get_price(&price.asset_id, today(), PriceKind::Close)
        .await?
        .expect("synchronizer price stored");
    assert_eq!(stored.price, "212.50");

    // Storing balances, transactions and the connection cursor all happen before
    // the report comes back.
    assert_eq!(storage.get_transactions(&account.id).await?.len(), 1);
    assert_eq!(
        storage
            .get_latest_balance_snapshot(&account.id)
            .await?
            .expect("snapshot saved")
            .balances
            .len(),
        2
    );
    assert_eq!(
        storage
            .get_connection(connection.id())
            .await?
            .expect("connection saved")
            .state
            .account_ids,
        vec![account.id.clone()]
    );
    Ok(())
}

#[tokio::test]
async fn valuation_prices_use_quotes_for_today_and_closes_for_past_dates() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let store = Arc::new(MemoryMarketDataStore::new());
    let source = Arc::new(CountingEquitySource::new("USD"));
    let orchestrator = orchestrator(
        storage,
        store,
        Some(Arc::new(EquityPriceRouter::new(vec![source.clone()]))),
        None,
        "USD",
    );

    let assets: HashSet<Asset> = [Asset::equity("AAPL")].into_iter().collect();

    let result = orchestrator
        .ensure_valuation_prices(&assets, today(), false)
        .await?;
    assert_eq!(result.fetched, 1);
    assert_eq!(source.quotes.load(Ordering::SeqCst), 1);
    assert_eq!(source.closes.load(Ordering::SeqCst), 0);

    let result = orchestrator
        .ensure_valuation_prices(&assets, today().pred_opt().unwrap(), false)
        .await?;
    assert_eq!(result.fetched, 1);
    assert_eq!(source.quotes.load(Ordering::SeqCst), 1);
    assert!(source.closes.load(Ordering::SeqCst) >= 1);
    Ok(())
}

#[tokio::test]
async fn a_cached_price_counts_as_skipped_rather_than_fetched() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let store = Arc::new(MemoryMarketDataStore::new());
    let asset = Asset::equity("AAPL");
    let yesterday = today().pred_opt().unwrap();
    store
        .put_prices(&[PricePoint {
            asset_id: AssetId::from_asset(&asset),
            as_of_date: yesterday,
            timestamp: Utc.with_ymd_and_hms(2026, 3, 1, 21, 0, 0).unwrap(),
            price: "100".to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "seed".to_string(),
        }])
        .await?;

    let source = Arc::new(CountingEquitySource::new("USD"));
    let orchestrator = orchestrator(
        storage,
        store,
        Some(Arc::new(EquityPriceRouter::new(vec![source.clone()]))),
        None,
        "USD",
    );

    let assets: HashSet<Asset> = [asset].into_iter().collect();
    let result = orchestrator
        .ensure_valuation_prices(&assets, yesterday, false)
        .await?;

    assert_eq!(result.skipped, 1);
    assert_eq!(result.fetched, 0);
    assert_eq!(source.closes.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn only_assets_quoted_outside_the_reporting_currency_need_an_fx_rate() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let store = Arc::new(MemoryMarketDataStore::new());
    let fx = Arc::new(CountingFxSource {
        calls: AtomicUsize::new(0),
    });
    let orchestrator = orchestrator(
        storage,
        store,
        Some(Arc::new(EquityPriceRouter::new(vec![Arc::new(
            CountingEquitySource::new("EUR"),
        )]))),
        Some(Arc::new(FxRateRouter::new(vec![fx.clone()]))),
        "USD",
    );

    let assets: HashSet<Asset> = [
        Asset::currency("USD"),
        Asset::currency("GBP"),
        Asset::manual_value("Car", "USD"),
        Asset::equity("SAP"),
    ]
    .into_iter()
    .collect();

    let result = orchestrator.ensure_prices(&assets, today(), false).await?;

    // GBP from the currency balance and EUR from the equity's quote currency.
    assert_eq!(fx.calls.load(Ordering::SeqCst), 2);
    assert_eq!(result.failed.len(), 0);
    Ok(())
}

#[tokio::test]
async fn a_price_that_cannot_be_fetched_is_reported_as_failed() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let store = Arc::new(MemoryMarketDataStore::new());
    let orchestrator = orchestrator(storage, store, None, None, "USD");

    let assets: HashSet<Asset> = [Asset::equity("AAPL")].into_iter().collect();
    let result = orchestrator.ensure_prices(&assets, today(), false).await?;

    assert_eq!(result.fetched, 0);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(result.failed[0].0, Asset::equity("AAPL"));
    Ok(())
}

#[tokio::test]
async fn refreshing_account_prices_only_looks_at_that_account() -> Result<()> {
    let storage = Arc::new(MemoryStorage::new());
    let store = Arc::new(MemoryMarketDataStore::new());
    let connection = connection();
    storage.save_connection(&connection).await?;

    let mut ids = Vec::new();
    for (index, symbol) in ["AAPL", "MSFT"].into_iter().enumerate() {
        let account = Account::new_with(
            Id::from_string(format!("acct-{index}")),
            Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            symbol,
            connection.id().clone(),
        );
        storage.save_account(&account).await?;
        storage
            .append_balance_snapshot(
                &account.id,
                &BalanceSnapshot::now(vec![AssetBalance::new(Asset::equity(symbol), "1")]),
            )
            .await?;
        ids.push(account.id);
    }

    let source = Arc::new(CountingEquitySource::new("USD"));
    let orchestrator = orchestrator(
        storage,
        store,
        Some(Arc::new(EquityPriceRouter::new(vec![source.clone()]))),
        None,
        "USD",
    );

    let result = orchestrator
        .refresh_account_valuation_prices(&ids[0], today(), false)
        .await?;
    assert_eq!(result.fetched, 1);
    assert_eq!(source.quotes.load(Ordering::SeqCst), 1);

    let result = orchestrator
        .refresh_connection_valuation_prices(connection.id(), today(), false)
        .await?;
    assert_eq!(result.fetched + result.skipped, 2);
    Ok(())
}

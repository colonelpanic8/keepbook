use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{TimeZone, Utc};

use super::*;
use crate::market_data::{FxRateKind, PriceKind};

#[derive(Debug, Clone, Copy)]
enum Outcome {
    Hit,
    Empty,
    Error,
}

struct RecordingSource {
    name: &'static str,
    close: Outcome,
    quote: Outcome,
    calls: Arc<AtomicUsize>,
    order: Arc<std::sync::Mutex<Vec<&'static str>>>,
}

impl RecordingSource {
    fn new(
        name: &'static str,
        close: Outcome,
        quote: Outcome,
        order: &Arc<std::sync::Mutex<Vec<&'static str>>>,
    ) -> Self {
        Self {
            name,
            close,
            quote,
            calls: Arc::new(AtomicUsize::new(0)),
            order: order.clone(),
        }
    }

    fn record(&self) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.order.lock().unwrap().push(self.name);
    }

    fn outcome(
        &self,
        outcome: Outcome,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        match outcome {
            Outcome::Hit => Ok(Some(price_point(self.name, asset_id, date))),
            Outcome::Empty => Ok(None),
            Outcome::Error => anyhow::bail!("{} is down", self.name),
        }
    }
}

fn quote_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 3, 2).unwrap()
}

#[async_trait::async_trait]
impl EquityPriceSource for RecordingSource {
    async fn fetch_close(
        &self,
        _asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        self.record();
        self.outcome(self.close, asset_id, date)
    }

    async fn fetch_quote(&self, _asset: &Asset, asset_id: &AssetId) -> Result<Option<PricePoint>> {
        self.record();
        self.outcome(self.quote, asset_id, quote_date())
    }

    fn name(&self) -> &str {
        self.name
    }
}

#[async_trait::async_trait]
impl CryptoPriceSource for RecordingSource {
    async fn fetch_close(
        &self,
        _asset: &Asset,
        asset_id: &AssetId,
        date: NaiveDate,
    ) -> Result<Option<PricePoint>> {
        self.record();
        self.outcome(self.close, asset_id, date)
    }

    async fn fetch_quote(&self, _asset: &Asset, asset_id: &AssetId) -> Result<Option<PricePoint>> {
        self.record();
        self.outcome(self.quote, asset_id, quote_date())
    }

    fn name(&self) -> &str {
        self.name
    }
}

struct RecordingFxSource {
    name: &'static str,
    outcome: Outcome,
    order: Arc<std::sync::Mutex<Vec<&'static str>>>,
}

#[async_trait::async_trait]
impl FxRateSource for RecordingFxSource {
    async fn fetch_close(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
    ) -> Result<Option<FxRatePoint>> {
        self.order.lock().unwrap().push(self.name);
        match self.outcome {
            Outcome::Hit => Ok(Some(FxRatePoint {
                base: base.to_string(),
                quote: quote.to_string(),
                as_of_date: date,
                timestamp: Utc.with_ymd_and_hms(2026, 3, 2, 21, 0, 0).unwrap(),
                rate: "1.25".to_string(),
                kind: FxRateKind::Close,
                source: self.name.to_string(),
            })),
            Outcome::Empty => Ok(None),
            Outcome::Error => anyhow::bail!("{} is down", self.name),
        }
    }

    fn name(&self) -> &str {
        self.name
    }
}

fn price_point(source: &str, asset_id: &AssetId, date: NaiveDate) -> PricePoint {
    PricePoint {
        asset_id: asset_id.clone(),
        as_of_date: date,
        timestamp: Utc.with_ymd_and_hms(2026, 3, 2, 21, 0, 0).unwrap(),
        price: "100".to_string(),
        quote_currency: "USD".to_string(),
        kind: PriceKind::Close,
        source: source.to_string(),
    }
}

fn equity_fixture() -> (Asset, AssetId, NaiveDate) {
    let asset = Asset::equity("AAPL");
    let asset_id = AssetId::from_asset(&asset);
    (
        asset,
        asset_id,
        NaiveDate::from_ymd_opt(2026, 3, 2).unwrap(),
    )
}

fn crypto_fixture() -> (Asset, AssetId, NaiveDate) {
    let asset = Asset::crypto("BTC");
    let asset_id = AssetId::from_asset(&asset);
    (
        asset,
        asset_id,
        NaiveDate::from_ymd_opt(2026, 3, 2).unwrap(),
    )
}

#[tokio::test]
async fn equity_close_skips_empty_and_failing_sources_in_order() -> Result<()> {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let empty = Arc::new(RecordingSource::new(
        "empty",
        Outcome::Empty,
        Outcome::Empty,
        &order,
    ));
    let failing = Arc::new(RecordingSource::new(
        "failing",
        Outcome::Error,
        Outcome::Error,
        &order,
    ));
    let good = Arc::new(RecordingSource::new(
        "good",
        Outcome::Hit,
        Outcome::Hit,
        &order,
    ));
    let last = Arc::new(RecordingSource::new(
        "last",
        Outcome::Hit,
        Outcome::Hit,
        &order,
    ));
    let router = EquityPriceRouter::new(vec![
        empty.clone(),
        failing.clone(),
        good.clone(),
        last.clone(),
    ]);

    let (asset, asset_id, date) = equity_fixture();
    let price = router.fetch_close(&asset, &asset_id, date).await?;

    assert_eq!(price.map(|p| p.source), Some("good".to_string()));
    assert_eq!(*order.lock().unwrap(), vec!["empty", "failing", "good"]);
    assert_eq!(last.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn equity_close_returns_none_when_every_source_misses() -> Result<()> {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let router = EquityPriceRouter::new(vec![
        Arc::new(RecordingSource::new(
            "failing",
            Outcome::Error,
            Outcome::Error,
            &order,
        )),
        Arc::new(RecordingSource::new(
            "empty",
            Outcome::Empty,
            Outcome::Empty,
            &order,
        )),
    ]);

    let (asset, asset_id, date) = equity_fixture();
    assert!(router.fetch_close(&asset, &asset_id, date).await?.is_none());
    assert_eq!(*order.lock().unwrap(), vec!["failing", "empty"]);
    Ok(())
}

#[tokio::test]
async fn equity_closes_treats_an_empty_range_as_a_miss() -> Result<()> {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let router = EquityPriceRouter::new(vec![
        Arc::new(RecordingSource::new(
            "empty",
            Outcome::Empty,
            Outcome::Empty,
            &order,
        )),
        Arc::new(RecordingSource::new(
            "good",
            Outcome::Hit,
            Outcome::Hit,
            &order,
        )),
    ]);

    let (asset, asset_id, date) = equity_fixture();
    let end = date.succ_opt().unwrap();
    let prices = router.fetch_closes(&asset, &asset_id, date, end).await?;

    assert_eq!(prices.len(), 2);
    assert!(prices.iter().all(|p| p.source == "good"));
    Ok(())
}

#[tokio::test]
async fn equity_quote_falls_through_to_a_supporting_source() -> Result<()> {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let router = EquityPriceRouter::new(vec![
        Arc::new(RecordingSource::new(
            "no-quotes",
            Outcome::Hit,
            Outcome::Empty,
            &order,
        )),
        Arc::new(RecordingSource::new(
            "quotes",
            Outcome::Empty,
            Outcome::Hit,
            &order,
        )),
    ]);

    let (asset, asset_id, _) = equity_fixture();
    let quote = router.fetch_quote(&asset, &asset_id).await?;

    assert_eq!(quote.map(|p| p.source), Some("quotes".to_string()));
    assert_eq!(*order.lock().unwrap(), vec!["no-quotes", "quotes"]);
    Ok(())
}

#[tokio::test]
async fn crypto_router_skips_empty_and_failing_sources() -> Result<()> {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let router = CryptoPriceRouter::new(vec![
        Arc::new(RecordingSource::new(
            "failing",
            Outcome::Error,
            Outcome::Error,
            &order,
        )),
        Arc::new(RecordingSource::new(
            "empty",
            Outcome::Empty,
            Outcome::Empty,
            &order,
        )),
        Arc::new(RecordingSource::new(
            "good",
            Outcome::Hit,
            Outcome::Hit,
            &order,
        )),
    ]);

    let (asset, asset_id, date) = crypto_fixture();
    let price = router.fetch_close(&asset, &asset_id, date).await?;
    assert_eq!(price.map(|p| p.source), Some("good".to_string()));

    let quote = router.fetch_quote(&asset, &asset_id).await?;
    assert_eq!(quote.map(|p| p.source), Some("good".to_string()));

    let range = router
        .fetch_closes(&asset, &asset_id, date, date)
        .await?
        .into_iter()
        .map(|p| p.source)
        .collect::<Vec<_>>();
    assert_eq!(range, vec!["good".to_string()]);
    Ok(())
}

#[tokio::test]
async fn fx_router_skips_empty_and_failing_sources() -> Result<()> {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let router = FxRateRouter::new(vec![
        Arc::new(RecordingFxSource {
            name: "failing",
            outcome: Outcome::Error,
            order: order.clone(),
        }),
        Arc::new(RecordingFxSource {
            name: "empty",
            outcome: Outcome::Empty,
            order: order.clone(),
        }),
        Arc::new(RecordingFxSource {
            name: "good",
            outcome: Outcome::Hit,
            order: order.clone(),
        }),
    ]);

    let date = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap();
    let rate = router.fetch_close("EUR", "USD", date).await?;

    assert_eq!(rate.map(|r| r.source), Some("good".to_string()));
    assert_eq!(*order.lock().unwrap(), vec!["failing", "empty", "good"]);
    Ok(())
}

#[tokio::test]
async fn fx_router_returns_none_when_every_source_misses() -> Result<()> {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let router = FxRateRouter::new(vec![Arc::new(RecordingFxSource {
        name: "failing",
        outcome: Outcome::Error,
        order: order.clone(),
    })]);

    let date = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap();
    assert!(router.fetch_close("EUR", "USD", date).await?.is_none());
    Ok(())
}

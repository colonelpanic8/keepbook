use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::{
    AssetId, AssetRegistryEntry, FxRateKind, FxRatePoint, MarketDataStore, PriceKind, PricePoint,
};

pub struct JsonlMarketDataStore {
    base_path: PathBuf,
}

impl JsonlMarketDataStore {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    fn assets_index_file(&self) -> PathBuf {
        self.base_path.join("assets").join("index.jsonl")
    }

    fn prices_dir(&self, asset_id: &AssetId) -> PathBuf {
        self.base_path
            .join("market")
            .join("prices")
            .join(asset_id.to_string())
    }

    fn fx_dir(&self, base: &str, quote: &str) -> PathBuf {
        let pair = format!("{}-{}", sanitize_code(base), sanitize_code(quote));
        self.base_path.join("market").join("fx").join(pair)
    }

    fn price_file(&self, asset_id: &AssetId, date: NaiveDate) -> PathBuf {
        self.prices_dir(asset_id)
            .join(format!("{:04}", date.year()))
            .join(format!("{:02}.jsonl", date.month()))
    }

    fn fx_file(&self, base: &str, quote: &str, date: NaiveDate) -> PathBuf {
        self.fx_dir(base, quote)
            .join(format!("{:04}", date.year()))
            .join(format!("{:02}.jsonl", date.month()))
    }

    async fn ensure_dir(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .context("Failed to create directory")?;
        }
        Ok(())
    }

    async fn read_jsonl<T: for<'de> serde::Deserialize<'de>>(&self, path: &Path) -> Result<Vec<T>> {
        let file = match fs::File::open(path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).context("Failed to open file"),
        };

        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut items = Vec::new();

        while let Some(line) = lines.next_line().await.context("Failed to read line")? {
            if line.trim().is_empty() {
                continue;
            }
            let item: T = serde_json::from_str(&line)
                .with_context(|| format!("Failed to parse JSONL line: {line}"))?;
            items.push(item);
        }

        Ok(items)
    }

    async fn append_jsonl<T: serde::Serialize>(&self, path: &Path, items: &[T]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        self.ensure_dir(path).await?;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await
            .context("Failed to open file for append")?;

        for item in items {
            let line = serde_json::to_string(item).context("Failed to serialize item")?;
            file.write_all(line.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }

        Ok(())
    }

    fn select_latest_price(
        &self,
        mut prices: Vec<PricePoint>,
        date: NaiveDate,
        kind: PriceKind,
    ) -> Option<PricePoint> {
        prices.retain(|p| p.as_of_date == date && p.kind == kind);
        prices.into_iter().max_by_key(|p| p.timestamp)
    }

    fn select_latest_fx(
        &self,
        mut rates: Vec<FxRatePoint>,
        date: NaiveDate,
        kind: FxRateKind,
    ) -> Option<FxRatePoint> {
        rates.retain(|r| r.as_of_date == date && r.kind == kind);
        rates.into_iter().max_by_key(|r| r.timestamp)
    }
}

#[async_trait::async_trait]
impl MarketDataStore for JsonlMarketDataStore {
    async fn get_price(
        &self,
        asset_id: &AssetId,
        date: NaiveDate,
        kind: PriceKind,
    ) -> Result<Option<PricePoint>> {
        let path = self.price_file(asset_id, date);
        let prices = self.read_jsonl(&path).await?;
        Ok(self.select_latest_price(prices, date, kind))
    }

    async fn put_prices(&self, prices: &[PricePoint]) -> Result<()> {
        if prices.is_empty() {
            return Ok(());
        }

        let mut grouped: std::collections::HashMap<(String, i32, u32), Vec<PricePoint>> =
            std::collections::HashMap::new();

        for price in prices {
            let key = (
                price.asset_id.to_string(),
                price.as_of_date.year(),
                price.as_of_date.month(),
            );
            grouped.entry(key).or_default().push(price.clone());
        }

        for ((asset_id, year, month), items) in grouped {
            let date = NaiveDate::from_ymd_opt(year, month, 1)
                .context("Invalid price date for storage")?;
            let path = self.price_file(&AssetId::from(asset_id), date);
            self.append_jsonl(&path, &items).await?;
        }

        Ok(())
    }

    async fn get_fx_rate(
        &self,
        base: &str,
        quote: &str,
        date: NaiveDate,
        kind: FxRateKind,
    ) -> Result<Option<FxRatePoint>> {
        let path = self.fx_file(base, quote, date);
        let rates = self.read_jsonl(&path).await?;
        Ok(self.select_latest_fx(rates, date, kind))
    }

    async fn put_fx_rates(&self, rates: &[FxRatePoint]) -> Result<()> {
        if rates.is_empty() {
            return Ok(());
        }

        let mut grouped: std::collections::HashMap<(String, String, i32, u32), Vec<FxRatePoint>> =
            std::collections::HashMap::new();

        for rate in rates {
            let key = (
                rate.base.clone(),
                rate.quote.clone(),
                rate.as_of_date.year(),
                rate.as_of_date.month(),
            );
            grouped.entry(key).or_default().push(rate.clone());
        }

        for ((base, quote, year, month), items) in grouped {
            let date = NaiveDate::from_ymd_opt(year, month, 1)
                .context("Invalid FX date for storage")?;
            let path = self.fx_file(&base, &quote, date);
            self.append_jsonl(&path, &items).await?;
        }

        Ok(())
    }

    async fn get_asset_entry(&self, asset_id: &AssetId) -> Result<Option<AssetRegistryEntry>> {
        let path = self.assets_index_file();
        let entries: Vec<AssetRegistryEntry> = self.read_jsonl(&path).await?;
        let entry = entries
            .into_iter()
            .rev()
            .find(|entry| entry.id == *asset_id);
        Ok(entry)
    }

    async fn upsert_asset_entry(&self, entry: &AssetRegistryEntry) -> Result<()> {
        let path = self.assets_index_file();
        self.append_jsonl(&path, &[entry]).await
    }
}

fn sanitize_code(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .to_uppercase()
}

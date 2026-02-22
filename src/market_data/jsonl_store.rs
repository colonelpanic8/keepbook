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
        self.base_path.join("prices").join(asset_id.to_string())
    }

    fn fx_dir(&self, base: &str, quote: &str) -> PathBuf {
        let pair = format!("{}-{}", sanitize_code(base), sanitize_code(quote));
        self.base_path.join("fx").join(pair)
    }

    fn price_file(&self, asset_id: &AssetId, date: NaiveDate) -> PathBuf {
        self.prices_dir(asset_id)
            .join(format!("{:04}.jsonl", date.year()))
    }

    fn fx_file(&self, base: &str, quote: &str, date: NaiveDate) -> PathBuf {
        self.fx_dir(base, quote)
            .join(format!("{:04}.jsonl", date.year()))
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

    async fn write_jsonl<T: serde::Serialize>(&self, path: &Path, items: &[T]) -> Result<()> {
        self.ensure_dir(path).await?;

        let mut content = String::new();
        for item in items {
            let line = serde_json::to_string(item).context("Failed to serialize item")?;
            content.push_str(&line);
            content.push('\n');
        }

        fs::write(path, content)
            .await
            .context("Failed to write JSONL file")?;
        Ok(())
    }

    fn price_kind_rank(kind: PriceKind) -> u8 {
        match kind {
            PriceKind::Close => 0,
            PriceKind::AdjClose => 1,
            PriceKind::Quote => 2,
        }
    }

    fn fx_kind_rank(kind: FxRateKind) -> u8 {
        match kind {
            FxRateKind::Close => 0,
        }
    }

    fn sort_prices(items: &mut [PricePoint]) {
        items.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.as_of_date.cmp(&b.as_of_date))
                .then_with(|| Self::price_kind_rank(a.kind).cmp(&Self::price_kind_rank(b.kind)))
                .then_with(|| a.quote_currency.cmp(&b.quote_currency))
                .then_with(|| a.source.cmp(&b.source))
                .then_with(|| a.price.cmp(&b.price))
                .then_with(|| a.asset_id.as_str().cmp(b.asset_id.as_str()))
        });
    }

    fn sort_fx_rates(items: &mut [FxRatePoint]) {
        items.sort_by(|a, b| {
            a.timestamp
                .cmp(&b.timestamp)
                .then_with(|| a.as_of_date.cmp(&b.as_of_date))
                .then_with(|| Self::fx_kind_rank(a.kind).cmp(&Self::fx_kind_rank(b.kind)))
                .then_with(|| a.base.cmp(&b.base))
                .then_with(|| a.quote.cmp(&b.quote))
                .then_with(|| a.source.cmp(&b.source))
                .then_with(|| a.rate.cmp(&b.rate))
        });
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

    async fn get_all_prices(&self, asset_id: &AssetId) -> Result<Vec<PricePoint>> {
        let prices_dir = self.prices_dir(asset_id);

        // List all year files in the prices directory
        let mut entries = match fs::read_dir(&prices_dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).context("Failed to read prices directory"),
        };

        let mut all_prices = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                let prices: Vec<PricePoint> = self.read_jsonl(&path).await?;
                all_prices.extend(prices);
            }
        }

        // Sort by timestamp for consistent ordering
        all_prices.sort_by_key(|p| p.timestamp);
        Ok(all_prices)
    }

    async fn put_prices(&self, prices: &[PricePoint]) -> Result<()> {
        if prices.is_empty() {
            return Ok(());
        }

        let mut grouped: std::collections::HashMap<(String, i32), Vec<PricePoint>> =
            std::collections::HashMap::new();

        for price in prices {
            let key = (price.asset_id.to_string(), price.as_of_date.year());
            grouped.entry(key).or_default().push(price.clone());
        }

        for ((asset_id, year), items) in grouped {
            let date =
                NaiveDate::from_ymd_opt(year, 1, 1).context("Invalid price date for storage")?;
            let path = self.price_file(&AssetId::from(asset_id), date);
            let mut all_items = self.read_jsonl::<PricePoint>(&path).await?;
            all_items.extend(items);
            Self::sort_prices(&mut all_items);
            self.write_jsonl(&path, &all_items).await?;
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

    async fn get_all_fx_rates(&self, base: &str, quote: &str) -> Result<Vec<FxRatePoint>> {
        let fx_dir = self.fx_dir(base, quote);

        // List all year files in the fx directory
        let mut entries = match fs::read_dir(&fx_dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).context("Failed to read FX directory"),
        };

        let mut all_rates = Vec::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                let rates: Vec<FxRatePoint> = self.read_jsonl(&path).await?;
                all_rates.extend(rates);
            }
        }

        // Sort by timestamp for consistent ordering
        all_rates.sort_by_key(|r| r.timestamp);
        Ok(all_rates)
    }

    async fn put_fx_rates(&self, rates: &[FxRatePoint]) -> Result<()> {
        if rates.is_empty() {
            return Ok(());
        }

        let mut grouped: std::collections::HashMap<(String, String, i32), Vec<FxRatePoint>> =
            std::collections::HashMap::new();

        for rate in rates {
            let key = (
                rate.base.clone(),
                rate.quote.clone(),
                rate.as_of_date.year(),
            );
            grouped.entry(key).or_default().push(rate.clone());
        }

        for ((base, quote, year), items) in grouped {
            let date =
                NaiveDate::from_ymd_opt(year, 1, 1).context("Invalid FX date for storage")?;
            let path = self.fx_file(&base, &quote, date);
            let mut all_items = self.read_jsonl::<FxRatePoint>(&path).await?;
            all_items.extend(items);
            Self::sort_fx_rates(&mut all_items);
            self.write_jsonl(&path, &all_items).await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Asset;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn make_price(as_of_date: &str, timestamp: chrono::DateTime<Utc>, price: &str) -> PricePoint {
        let asset = Asset::equity("AAPL");
        PricePoint {
            asset_id: AssetId::from_asset(&asset),
            as_of_date: NaiveDate::parse_from_str(as_of_date, "%Y-%m-%d").unwrap(),
            timestamp,
            price: price.to_string(),
            quote_currency: "USD".to_string(),
            kind: PriceKind::Close,
            source: "test".to_string(),
        }
    }

    fn make_fx(as_of_date: &str, timestamp: chrono::DateTime<Utc>, rate: &str) -> FxRatePoint {
        FxRatePoint {
            base: "USD".to_string(),
            quote: "EUR".to_string(),
            as_of_date: NaiveDate::parse_from_str(as_of_date, "%Y-%m-%d").unwrap(),
            timestamp,
            rate: rate.to_string(),
            kind: FxRateKind::Close,
            source: "test".to_string(),
        }
    }

    #[tokio::test]
    async fn put_prices_rewrites_year_file_in_chronological_order() -> Result<()> {
        let base_path = std::env::temp_dir().join(format!("keepbook-md-{}", Uuid::new_v4()));
        fs::create_dir_all(&base_path).await?;
        let store = JsonlMarketDataStore::new(&base_path);

        let newer = make_price(
            "2024-12-31",
            Utc.with_ymd_and_hms(2024, 12, 31, 21, 0, 0).unwrap(),
            "250.00",
        );
        let older = make_price(
            "2024-01-15",
            Utc.with_ymd_and_hms(2024, 1, 15, 21, 0, 0).unwrap(),
            "180.00",
        );

        store.put_prices(&[newer]).await?;
        store.put_prices(&[older]).await?;

        let path = store.price_file(
            &AssetId::from_asset(&Asset::equity("AAPL")),
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        );
        let lines = fs::read_to_string(&path).await?;
        let parsed: Vec<PricePoint> = lines
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0].as_of_date,
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
        );
        assert_eq!(
            parsed[1].as_of_date,
            NaiveDate::from_ymd_opt(2024, 12, 31).unwrap()
        );

        let _ = fs::remove_dir_all(&base_path).await;
        Ok(())
    }

    #[tokio::test]
    async fn put_fx_rates_rewrites_year_file_in_chronological_order() -> Result<()> {
        let base_path = std::env::temp_dir().join(format!("keepbook-md-{}", Uuid::new_v4()));
        fs::create_dir_all(&base_path).await?;
        let store = JsonlMarketDataStore::new(&base_path);

        let newer = make_fx(
            "2024-12-31",
            Utc.with_ymd_and_hms(2024, 12, 31, 18, 0, 0).unwrap(),
            "0.9900",
        );
        let older = make_fx(
            "2024-01-15",
            Utc.with_ymd_and_hms(2024, 1, 15, 18, 0, 0).unwrap(),
            "0.9100",
        );

        store.put_fx_rates(&[newer]).await?;
        store.put_fx_rates(&[older]).await?;

        let path = store.fx_file("USD", "EUR", NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
        let lines = fs::read_to_string(&path).await?;
        let parsed: Vec<FxRatePoint> = lines
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0].as_of_date,
            NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
        );
        assert_eq!(
            parsed[1].as_of_date,
            NaiveDate::from_ymd_opt(2024, 12, 31).unwrap()
        );

        let _ = fs::remove_dir_all(&base_path).await;
        Ok(())
    }
}

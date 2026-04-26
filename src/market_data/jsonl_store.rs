use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::{
    AssetId, AssetRegistryEntry, FxRateKind, FxRatePoint, MarketDataStore, PriceKind, PricePoint,
};

pub struct JsonlMarketDataStore {
    base_path: PathBuf,
    cache: Arc<Mutex<JsonlMarketDataCache>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FsCacheKey {
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug, Clone)]
struct CachedRead<T> {
    key: Option<FsCacheKey>,
    value: T,
}

#[derive(Default)]
struct JsonlMarketDataCache {
    price_files: HashMap<PathBuf, CachedRead<Vec<PricePoint>>>,
    price_dirs: HashMap<PathBuf, CachedRead<Vec<PathBuf>>>,
    fx_files: HashMap<PathBuf, CachedRead<Vec<FxRatePoint>>>,
    fx_dirs: HashMap<PathBuf, CachedRead<Vec<PathBuf>>>,
    asset_index: Option<CachedRead<HashMap<AssetId, AssetRegistryEntry>>>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct MarketDataJsonlNormalizationStats {
    pub price_files_rewritten: usize,
    pub fx_files_rewritten: usize,
    pub price_points_sorted: usize,
    pub fx_rate_points_sorted: usize,
}

impl JsonlMarketDataStore {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
            cache: Arc::new(Mutex::new(JsonlMarketDataCache::default())),
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

    async fn fs_cache_key(path: &Path) -> Result<Option<FsCacheKey>> {
        match fs::metadata(path).await {
            Ok(metadata) => Ok(Some(FsCacheKey {
                len: metadata.len(),
                modified: metadata.modified().ok(),
            })),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("Failed to stat {}", path.display())),
        }
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
            a.as_of_date
                .cmp(&b.as_of_date)
                .then_with(|| a.timestamp.cmp(&b.timestamp))
                .then_with(|| Self::price_kind_rank(a.kind).cmp(&Self::price_kind_rank(b.kind)))
                .then_with(|| a.quote_currency.cmp(&b.quote_currency))
                .then_with(|| a.source.cmp(&b.source))
                .then_with(|| a.price.cmp(&b.price))
                .then_with(|| a.asset_id.as_str().cmp(b.asset_id.as_str()))
        });
    }

    fn sort_fx_rates(items: &mut [FxRatePoint]) {
        items.sort_by(|a, b| {
            a.as_of_date
                .cmp(&b.as_of_date)
                .then_with(|| a.timestamp.cmp(&b.timestamp))
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

    async fn read_cached_price_file(&self, path: &Path) -> Result<Vec<PricePoint>> {
        let key = Self::fs_cache_key(path).await?;
        {
            let cache = self.cache.lock().expect("market data cache poisoned");
            if let Some(cached) = cache.price_files.get(path) {
                if cached.key == key {
                    return Ok(cached.value.clone());
                }
            }
        }

        let mut prices = self.read_jsonl::<PricePoint>(path).await?;
        Self::sort_prices(&mut prices);
        let value = prices.clone();
        self.cache
            .lock()
            .expect("market data cache poisoned")
            .price_files
            .insert(path.to_path_buf(), CachedRead { key, value });
        Ok(prices)
    }

    async fn read_cached_fx_file(&self, path: &Path) -> Result<Vec<FxRatePoint>> {
        let key = Self::fs_cache_key(path).await?;
        {
            let cache = self.cache.lock().expect("market data cache poisoned");
            if let Some(cached) = cache.fx_files.get(path) {
                if cached.key == key {
                    return Ok(cached.value.clone());
                }
            }
        }

        let mut rates = self.read_jsonl::<FxRatePoint>(path).await?;
        Self::sort_fx_rates(&mut rates);
        let value = rates.clone();
        self.cache
            .lock()
            .expect("market data cache poisoned")
            .fx_files
            .insert(path.to_path_buf(), CachedRead { key, value });
        Ok(rates)
    }

    async fn list_cached_jsonl_files(&self, dir: &Path, is_fx: bool) -> Result<Vec<PathBuf>> {
        let key = Self::fs_cache_key(dir).await?;
        {
            let cache = self.cache.lock().expect("market data cache poisoned");
            let cached = if is_fx {
                cache.fx_dirs.get(dir)
            } else {
                cache.price_dirs.get(dir)
            };
            if let Some(cached) = cached {
                if cached.key == key {
                    return Ok(cached.value.clone());
                }
            }
        }

        let mut entries = match fs::read_dir(dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let value = Vec::new();
                let mut cache = self.cache.lock().expect("market data cache poisoned");
                let target = if is_fx {
                    &mut cache.fx_dirs
                } else {
                    &mut cache.price_dirs
                };
                target.insert(dir.to_path_buf(), CachedRead { key, value });
                return Ok(Vec::new());
            }
            Err(e) => return Err(e).context("Failed to read market data directory"),
        };

        let mut files = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                files.push(path);
            }
        }
        files.sort();

        let value = files.clone();
        let mut cache = self.cache.lock().expect("market data cache poisoned");
        let target = if is_fx {
            &mut cache.fx_dirs
        } else {
            &mut cache.price_dirs
        };
        target.insert(dir.to_path_buf(), CachedRead { key, value });
        Ok(files)
    }

    async fn read_cached_asset_index(&self) -> Result<HashMap<AssetId, AssetRegistryEntry>> {
        let path = self.assets_index_file();
        let key = Self::fs_cache_key(&path).await?;
        {
            let cache = self.cache.lock().expect("market data cache poisoned");
            if let Some(cached) = &cache.asset_index {
                if cached.key == key {
                    return Ok(cached.value.clone());
                }
            }
        }

        let entries: Vec<AssetRegistryEntry> = self.read_jsonl(&path).await?;
        let mut by_id = HashMap::new();
        for entry in entries {
            by_id.insert(entry.id.clone(), entry);
        }

        let value = by_id.clone();
        self.cache
            .lock()
            .expect("market data cache poisoned")
            .asset_index = Some(CachedRead { key, value });
        Ok(by_id)
    }

    async fn cache_price_file(&self, path: &Path, prices: &[PricePoint]) -> Result<()> {
        let key = Self::fs_cache_key(path).await?;
        self.cache
            .lock()
            .expect("market data cache poisoned")
            .price_files
            .insert(
                path.to_path_buf(),
                CachedRead {
                    key,
                    value: prices.to_vec(),
                },
            );
        Ok(())
    }

    async fn cache_fx_file(&self, path: &Path, rates: &[FxRatePoint]) -> Result<()> {
        let key = Self::fs_cache_key(path).await?;
        self.cache
            .lock()
            .expect("market data cache poisoned")
            .fx_files
            .insert(
                path.to_path_buf(),
                CachedRead {
                    key,
                    value: rates.to_vec(),
                },
            );
        Ok(())
    }

    fn invalidate_price_dir(&self, asset_id: &AssetId) {
        let dir = self.prices_dir(asset_id);
        self.cache
            .lock()
            .expect("market data cache poisoned")
            .price_dirs
            .remove(&dir);
    }

    fn invalidate_fx_dir(&self, base: &str, quote: &str) {
        let dir = self.fx_dir(base, quote);
        self.cache
            .lock()
            .expect("market data cache poisoned")
            .fx_dirs
            .remove(&dir);
    }

    fn clear_cache(&self) {
        *self.cache.lock().expect("market data cache poisoned") = JsonlMarketDataCache::default();
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
        let prices = self.read_cached_price_file(&path).await?;
        Ok(self.select_latest_price(prices, date, kind))
    }

    async fn get_all_prices(&self, asset_id: &AssetId) -> Result<Vec<PricePoint>> {
        let prices_dir = self.prices_dir(asset_id);
        let mut all_prices = Vec::new();

        for path in self.list_cached_jsonl_files(&prices_dir, false).await? {
            let prices = self.read_cached_price_file(&path).await?;
            all_prices.extend(prices);
        }

        Self::sort_prices(&mut all_prices);
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
            let asset_id = AssetId::from(asset_id);
            let path = self.price_file(&asset_id, date);
            let mut all_items = self.read_cached_price_file(&path).await?;
            all_items.extend(items);
            Self::sort_prices(&mut all_items);
            self.write_jsonl(&path, &all_items).await?;
            self.cache_price_file(&path, &all_items).await?;
            self.invalidate_price_dir(&asset_id);
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
        let rates = self.read_cached_fx_file(&path).await?;
        Ok(self.select_latest_fx(rates, date, kind))
    }

    async fn get_all_fx_rates(&self, base: &str, quote: &str) -> Result<Vec<FxRatePoint>> {
        let fx_dir = self.fx_dir(base, quote);
        let mut all_rates = Vec::new();

        for path in self.list_cached_jsonl_files(&fx_dir, true).await? {
            let rates = self.read_cached_fx_file(&path).await?;
            all_rates.extend(rates);
        }

        Self::sort_fx_rates(&mut all_rates);
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
            let mut all_items = self.read_cached_fx_file(&path).await?;
            all_items.extend(items);
            Self::sort_fx_rates(&mut all_items);
            self.write_jsonl(&path, &all_items).await?;
            self.cache_fx_file(&path, &all_items).await?;
            self.invalidate_fx_dir(&base, &quote);
        }

        Ok(())
    }

    async fn get_asset_entry(&self, asset_id: &AssetId) -> Result<Option<AssetRegistryEntry>> {
        Ok(self.read_cached_asset_index().await?.get(asset_id).cloned())
    }

    async fn upsert_asset_entry(&self, entry: &AssetRegistryEntry) -> Result<()> {
        let path = self.assets_index_file();
        self.append_jsonl(&path, &[entry]).await?;
        self.cache
            .lock()
            .expect("market data cache poisoned")
            .asset_index = None;
        Ok(())
    }
}

impl JsonlMarketDataStore {
    async fn collect_jsonl_files(dir: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        let mut dirs = vec![dir.to_path_buf()];

        while let Some(dir_path) = dirs.pop() {
            let mut entries = match fs::read_dir(&dir_path).await {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e).context("Failed to read directory"),
            };

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let file_type = entry.file_type().await?;
                if file_type.is_dir() {
                    dirs.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    files.push(path);
                }
            }
        }

        Ok(files)
    }

    pub async fn recompact_all_jsonl(&self) -> Result<MarketDataJsonlNormalizationStats> {
        let mut stats = MarketDataJsonlNormalizationStats::default();

        for path in Self::collect_jsonl_files(&self.base_path.join("prices")).await? {
            let mut prices = self.read_jsonl::<PricePoint>(&path).await?;
            stats.price_points_sorted += prices.len();
            Self::sort_prices(&mut prices);
            self.write_jsonl(&path, &prices).await?;
            stats.price_files_rewritten += 1;
        }

        for path in Self::collect_jsonl_files(&self.base_path.join("fx")).await? {
            let mut rates = self.read_jsonl::<FxRatePoint>(&path).await?;
            stats.fx_rate_points_sorted += rates.len();
            Self::sort_fx_rates(&mut rates);
            self.write_jsonl(&path, &rates).await?;
            stats.fx_files_rewritten += 1;
        }

        self.clear_cache();
        Ok(stats)
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

    #[tokio::test]
    async fn asset_registry_cache_reads_index_as_one_file_and_refreshes_on_change() -> Result<()> {
        let base_path = std::env::temp_dir().join(format!("keepbook-md-{}", Uuid::new_v4()));
        fs::create_dir_all(&base_path).await?;
        let store = JsonlMarketDataStore::new(&base_path);

        let aapl = AssetRegistryEntry::new(Asset::equity("AAPL"));
        let msft = AssetRegistryEntry::new(Asset::equity("MSFT"));
        store.upsert_asset_entry(&aapl).await?;

        assert!(store.get_asset_entry(&aapl.id).await?.is_some());
        assert_eq!(
            store
                .cache
                .lock()
                .expect("market data cache poisoned")
                .asset_index
                .as_ref()
                .expect("asset index cached")
                .value
                .len(),
            1
        );

        let path = store.assets_index_file();
        let mut file = fs::OpenOptions::new().append(true).open(&path).await?;
        file.write_all(serde_json::to_string(&msft)?.as_bytes())
            .await?;
        file.write_all(b"\n").await?;

        assert!(store.get_asset_entry(&msft.id).await?.is_some());
        assert_eq!(
            store
                .cache
                .lock()
                .expect("market data cache poisoned")
                .asset_index
                .as_ref()
                .expect("asset index cached")
                .value
                .len(),
            2
        );

        let _ = fs::remove_dir_all(&base_path).await;
        Ok(())
    }

    #[tokio::test]
    async fn put_prices_orders_by_as_of_date_before_timestamp() -> Result<()> {
        let base_path = std::env::temp_dir().join(format!("keepbook-md-{}", Uuid::new_v4()));
        fs::create_dir_all(&base_path).await?;
        let store = JsonlMarketDataStore::new(&base_path);

        let next_day = make_price(
            "2024-04-08",
            Utc.with_ymd_and_hms(2024, 4, 8, 16, 20, 47).unwrap(),
            "197.67",
        );
        let late_backfill = make_price(
            "2024-04-07",
            Utc.with_ymd_and_hms(2024, 4, 8, 16, 27, 35).unwrap(),
            "193.49",
        );

        store.put_prices(&[next_day, late_backfill]).await?;

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
        assert_eq!(
            parsed.iter().map(|p| p.as_of_date).collect::<Vec<_>>(),
            vec![
                NaiveDate::from_ymd_opt(2024, 4, 7).unwrap(),
                NaiveDate::from_ymd_opt(2024, 4, 8).unwrap(),
            ]
        );

        let _ = fs::remove_dir_all(&base_path).await;
        Ok(())
    }

    #[tokio::test]
    async fn recompact_all_jsonl_resorts_market_data_files() -> Result<()> {
        let base_path = std::env::temp_dir().join(format!("keepbook-md-{}", Uuid::new_v4()));
        fs::create_dir_all(&base_path).await?;
        let store = JsonlMarketDataStore::new(&base_path);

        let asset_id = AssetId::from_asset(&Asset::equity("AAPL"));
        let price_path = store.price_file(&asset_id, NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
        store
            .write_jsonl(
                &price_path,
                &[
                    make_price(
                        "2024-04-08",
                        Utc.with_ymd_and_hms(2024, 4, 8, 16, 20, 47).unwrap(),
                        "197.67",
                    ),
                    make_price(
                        "2024-04-07",
                        Utc.with_ymd_and_hms(2024, 4, 8, 16, 27, 35).unwrap(),
                        "193.49",
                    ),
                ],
            )
            .await?;

        let fx_path = store.fx_file("USD", "EUR", NaiveDate::from_ymd_opt(2024, 1, 1).unwrap());
        store
            .write_jsonl(
                &fx_path,
                &[
                    make_fx(
                        "2024-04-08",
                        Utc.with_ymd_and_hms(2024, 4, 8, 16, 20, 47).unwrap(),
                        "0.93",
                    ),
                    make_fx(
                        "2024-04-07",
                        Utc.with_ymd_and_hms(2024, 4, 8, 16, 27, 35).unwrap(),
                        "0.92",
                    ),
                ],
            )
            .await?;

        let stats = store.recompact_all_jsonl().await?;
        assert_eq!(
            stats,
            MarketDataJsonlNormalizationStats {
                price_files_rewritten: 1,
                fx_files_rewritten: 1,
                price_points_sorted: 2,
                fx_rate_points_sorted: 2,
            }
        );

        let prices: Vec<PricePoint> = store.read_jsonl(&price_path).await?;
        assert_eq!(
            prices.iter().map(|p| p.as_of_date).collect::<Vec<_>>(),
            vec![
                NaiveDate::from_ymd_opt(2024, 4, 7).unwrap(),
                NaiveDate::from_ymd_opt(2024, 4, 8).unwrap(),
            ]
        );

        let rates: Vec<FxRatePoint> = store.read_jsonl(&fx_path).await?;
        assert_eq!(
            rates.iter().map(|r| r.as_of_date).collect::<Vec<_>>(),
            vec![
                NaiveDate::from_ymd_opt(2024, 4, 7).unwrap(),
                NaiveDate::from_ymd_opt(2024, 4, 8).unwrap(),
            ]
        );

        let _ = fs::remove_dir_all(&base_path).await;
        Ok(())
    }
}

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

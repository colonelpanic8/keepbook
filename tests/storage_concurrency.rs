use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::{TimeZone, Utc};
use keepbook::market_data::{
    AssetId, AssetRegistryEntry, FxRateKind, FxRatePoint, JsonlMarketDataStore, MarketDataStore,
    PriceKind, PricePoint,
};
use keepbook::models::{Account, Asset, AssetBalance, BalanceSnapshot, Id, Transaction};
use keepbook::storage::{JsonFileStorage, Storage};

fn price(index: u32) -> PricePoint {
    let timestamp = Utc.with_ymd_and_hms(2026, 2, 6, 18, 0, 0).unwrap()
        + chrono::Duration::seconds(i64::from(index));
    PricePoint {
        asset_id: AssetId::from_asset(&Asset::equity("AAPL")),
        as_of_date: timestamp.date_naive(),
        timestamp,
        price: index.to_string(),
        quote_currency: "USD".into(),
        kind: PriceKind::Quote,
        source: "test".into(),
    }
}

#[tokio::test]
async fn concurrent_market_data_writes_preserve_all_observations() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let shared = Arc::new(JsonlMarketDataStore::new(dir.path()));
    let barrier = Arc::new(tokio::sync::Barrier::new(16));
    let mut tasks = Vec::new();
    for index in 0..16 {
        let store = if index % 2 == 0 {
            shared.clone()
        } else {
            Arc::new(JsonlMarketDataStore::new(dir.path()))
        };
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            store.put_prices(&[price(index)]).await
        }));
    }
    for task in tasks {
        task.await??;
    }
    let reader = JsonlMarketDataStore::new(dir.path());
    assert_eq!(reader.get_all_prices(&price(0).asset_id).await?.len(), 16);
    assert_eq!(shared.get_all_prices(&price(0).asset_id).await?.len(), 16);
    Ok(())
}

#[tokio::test]
async fn persistence_worker() -> Result<()> {
    let Ok(root) = std::env::var("KEEPBOOK_PERSISTENCE_TEST_ROOT") else {
        return Ok(());
    };
    let worker: u32 = std::env::var("KEEPBOOK_PERSISTENCE_TEST_WORKER")?.parse()?;
    let root = std::path::PathBuf::from(root);
    tokio::fs::write(root.join(format!("ready-{worker}")), b"").await?;
    tokio::time::timeout(Duration::from_secs(20), async {
        while !root.join("start").exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await?;
    let market = JsonlMarketDataStore::new(&root);
    let storage = JsonFileStorage::new(&root);
    let account_id = Id::from("shared");
    for offset in 0..8 {
        let index = worker * 8 + offset;
        let point = price(index);
        market.put_prices(std::slice::from_ref(&point)).await?;
        market
            .put_fx_rates(&[FxRatePoint {
                base: "USD".into(),
                quote: "EUR".into(),
                as_of_date: point.as_of_date,
                timestamp: point.timestamp,
                rate: "0.9".into(),
                kind: FxRateKind::Close,
                source: index.to_string(),
            }])
            .await?;
        market
            .upsert_asset_entry(&AssetRegistryEntry::new(Asset::equity(format!(
                "ASSET{index}"
            ))))
            .await?;
        let mut transaction =
            Transaction::new("-1", Asset::currency("USD"), "payload".repeat(2048));
        transaction.id = Id::from(format!("tx-{index}"));
        transaction.timestamp = point.timestamp;
        storage
            .append_transactions(&account_id, &[transaction])
            .await?;
        storage
            .append_balance_snapshot(
                &account_id,
                &BalanceSnapshot::new(
                    point.timestamp,
                    vec![AssetBalance::new(Asset::currency("USD"), index.to_string())],
                ),
            )
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn processes_append_safely_while_other_stores_read_and_compact() -> Result<()> {
    tokio::time::timeout(Duration::from_secs(30), async {
        let dir = tempfile::tempdir()?;
        let storage = JsonFileStorage::new(dir.path());
        let account = Account::new_with(
            Id::from("shared"),
            price(0).timestamp,
            "Shared",
            Id::from("connection"),
        );
        storage.save_account(&account).await?;
        let market = JsonlMarketDataStore::new(dir.path());
        let mut children = Vec::new();
        for worker in 0..3 {
            children.push(
                tokio::process::Command::new(std::env::current_exe()?)
                    .args(["--exact", "persistence_worker", "--nocapture"])
                    .env("KEEPBOOK_PERSISTENCE_TEST_ROOT", dir.path())
                    .env("KEEPBOOK_PERSISTENCE_TEST_WORKER", worker.to_string())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::piped())
                    .kill_on_drop(true)
                    .spawn()?,
            );
        }
        while !(0..3).all(|worker| dir.path().join(format!("ready-{worker}")).exists()) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        tokio::fs::write(dir.path().join("start"), b"").await?;
        for _ in 0..24 {
            market.recompact_all_jsonl().await?;
            storage.recompact_all_jsonl().await?;
            storage.backfill_transaction_metadata_all().await?;
            storage.get_transactions(&account.id).await?;
            storage.get_balance_snapshots(&account.id).await?;
            market.get_all_prices(&price(0).asset_id).await?;
            tokio::task::yield_now().await;
        }
        for child in children {
            let output = child.wait_with_output().await?;
            anyhow::ensure!(
                output.status.success(),
                "child failed: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert_eq!(market.get_all_prices(&price(0).asset_id).await?.len(), 24);
        assert_eq!(market.get_all_fx_rates("USD", "EUR").await?.len(), 24);
        assert_eq!(storage.get_transactions(&account.id).await?.len(), 24);
        assert_eq!(storage.get_balance_snapshots(&account.id).await?.len(), 24);
        for index in 0..24 {
            let id = AssetId::from_asset(&Asset::equity(format!("ASSET{index}")));
            assert!(market.get_asset_entry(&id).await?.is_some());
        }
        Ok::<_, anyhow::Error>(())
    })
    .await?
}

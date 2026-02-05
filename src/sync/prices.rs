use anyhow::Result;

use crate::market_data::MarketDataStore;

use super::SyncResult;

/// Persist any prices included in a [`SyncResult`] (typically provided by synchronizers).
pub async fn store_sync_prices(result: &SyncResult, store: &dyn MarketDataStore) -> Result<usize> {
    let mut count = 0usize;

    for (_, synced_balances) in &result.balances {
        for sb in synced_balances {
            if let Some(price) = &sb.price {
                store.put_prices(std::slice::from_ref(price)).await?;
                count += 1;
            }
        }
    }

    Ok(count)
}


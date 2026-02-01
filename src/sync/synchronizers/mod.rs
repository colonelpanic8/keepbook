//! Synchronizer implementations for various financial services.

mod coinbase;
mod schwab;

pub use coinbase::CoinbaseSynchronizer;
pub use schwab::SchwabSynchronizer;

use anyhow::{anyhow, Result};

use crate::models::Connection;
use crate::storage::Storage;

use super::Synchronizer;

/// Create a synchronizer for the given connection.
///
/// The synchronizer type is determined by `connection.config.synchronizer`.
pub async fn create_synchronizer<S: Storage>(
    connection: &Connection,
    storage: &S,
) -> Result<Box<dyn Synchronizer>> {
    match connection.config.synchronizer.as_str() {
        "schwab" => Ok(Box::new(SchwabSynchronizer::from_connection(connection, storage).await?)),
        "coinbase" => Ok(Box::new(CoinbaseSynchronizer::from_connection(connection, storage).await?)),
        other => Err(anyhow!("Unknown synchronizer: {}", other)),
    }
}

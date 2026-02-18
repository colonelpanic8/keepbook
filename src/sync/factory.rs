use std::path::PathBuf;

use anyhow::{anyhow, Result};

use crate::models::Connection;
use crate::storage::Storage;

use super::Synchronizer;
use crate::sync::synchronizers::{
    ChaseSynchronizer, CoinbaseSynchronizer, PlaidSynchronizer, SchwabSynchronizer,
};

#[async_trait::async_trait]
pub trait SynchronizerFactory: Send + Sync {
    async fn create(
        &self,
        connection: &Connection,
        storage: &dyn Storage,
    ) -> Result<Box<dyn Synchronizer>>;
}

#[derive(Debug, Clone, Default)]
pub struct DefaultSynchronizerFactory {
    data_dir: Option<PathBuf>,
}

impl DefaultSynchronizerFactory {
    pub fn new(data_dir: Option<PathBuf>) -> Self {
        Self { data_dir }
    }
}

#[async_trait::async_trait]
impl SynchronizerFactory for DefaultSynchronizerFactory {
    async fn create(
        &self,
        connection: &Connection,
        storage: &dyn Storage,
    ) -> Result<Box<dyn Synchronizer>> {
        match connection.config.synchronizer.as_str() {
            "chase" => {
                // Chase uses ephemeral cache directories for browser profiles/downloads.
                // Do not root these in the keepbook data dir (which may be a git repo).
                let _ = &self.data_dir;
                Ok(Box::new(
                    ChaseSynchronizer::from_connection(connection, storage).await?,
                ))
            }
            "schwab" => Ok(Box::new(
                SchwabSynchronizer::from_connection(connection, storage).await?,
            )),
            "coinbase" => Ok(Box::new(
                CoinbaseSynchronizer::from_connection(connection, storage).await?,
            )),
            "plaid" => Ok(Box::new(
                PlaidSynchronizer::from_connection(connection, storage).await?,
            )),
            other => Err(anyhow!("Unknown synchronizer: {other}")),
        }
    }
}

pub async fn create_synchronizer<S: Storage>(
    connection: &Connection,
    storage: &S,
) -> Result<Box<dyn Synchronizer>> {
    DefaultSynchronizerFactory::new(None)
        .create(connection, storage)
        .await
}

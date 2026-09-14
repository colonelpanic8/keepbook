use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct GitSyncInput {
    pub(crate) data_dir: String,
    pub(crate) host: String,
    pub(crate) repo: String,
    pub(crate) branch: String,
    pub(crate) ssh_user: String,
    pub(crate) private_key_pem: String,
    pub(crate) save_settings: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct GitSyncOutput {
    pub(crate) ok: bool,
    pub(crate) data_dir: String,
    pub(crate) remote_url: String,
    pub(crate) branch: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct SyncConnectionsInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    pub(crate) if_stale: bool,
    pub(crate) full_transactions: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct SyncPricesInput {
    pub(crate) scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target: Option<String>,
    pub(crate) force: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) quote_staleness_seconds: Option<u64>,
}

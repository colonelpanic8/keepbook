use std::fmt;

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::models::Asset;

/// Stable, path-safe identifier for assets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(String);

impl AssetId {
    pub fn from_asset(asset: &Asset) -> Self {
        let canonical = canonical_asset_json(asset);
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let digest = hasher.finalize();
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
        Self(encoded)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for AssetId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for AssetId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl AsRef<str> for AssetId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

fn canonical_asset_json(asset: &Asset) -> String {
    use serde_json::{Map, Value};

    let mut map = Map::new();

    match asset {
        Asset::Currency { iso_code } => {
            map.insert("type".to_string(), Value::String("currency".to_string()));
            map.insert(
                "iso_code".to_string(),
                Value::String(normalize_upper(iso_code)),
            );
        }
        Asset::Equity { ticker, exchange } => {
            map.insert("type".to_string(), Value::String("equity".to_string()));
            map.insert(
                "ticker".to_string(),
                Value::String(normalize_upper(ticker)),
            );
            if let Some(exchange) = exchange {
                map.insert(
                    "exchange".to_string(),
                    Value::String(normalize_upper(exchange)),
                );
            }
        }
        Asset::Crypto { symbol, network } => {
            map.insert("type".to_string(), Value::String("crypto".to_string()));
            map.insert(
                "symbol".to_string(),
                Value::String(normalize_upper(symbol)),
            );
            if let Some(network) = network {
                map.insert(
                    "network".to_string(),
                    Value::String(normalize_upper(network)),
                );
            }
        }
    }

    let value = Value::Object(map);
    serde_json::to_string(&value).expect("Failed to serialize canonical asset JSON")
}

fn normalize_upper(value: &str) -> String {
    value.trim().to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Asset;

    #[test]
    fn asset_id_is_deterministic() {
        let asset = Asset::equity("AAPL");
        let first = AssetId::from_asset(&asset);
        let second = AssetId::from_asset(&asset);
        assert_eq!(first, second);
    }

    #[test]
    fn asset_id_differs_for_distinct_assets() {
        let aapl = Asset::equity("AAPL");
        let msft = Asset::equity("MSFT");
        let aapl_id = AssetId::from_asset(&aapl);
        let msft_id = AssetId::from_asset(&msft);
        assert_ne!(aapl_id, msft_id);
    }

    #[test]
    fn canonicalization_normalizes_case() {
        let asset = Asset::Currency {
            iso_code: "usd".to_string(),
        };
        let id_lower = AssetId::from_asset(&asset);
        let asset_upper = Asset::Currency {
            iso_code: "USD".to_string(),
        };
        let id_upper = AssetId::from_asset(&asset_upper);
        assert_eq!(id_lower, id_upper);
    }
}

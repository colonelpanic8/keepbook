use std::fmt;

use serde::{Deserialize, Serialize};

use crate::models::Asset;

/// Stable, path-safe identifier for assets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AssetId(String);

impl AssetId {
    pub fn from_asset(asset: &Asset) -> Self {
        let id = match asset {
            Asset::Currency { iso_code } => {
                format!("currency/{}", iso_code.trim().to_uppercase())
            }
            Asset::Equity { ticker, exchange: None } => {
                format!("equity/{}", ticker.trim().to_uppercase())
            }
            Asset::Equity { ticker, exchange: Some(ex) } => {
                format!("equity/{}/{}", ticker.trim().to_uppercase(), ex.trim().to_uppercase())
            }
            Asset::Crypto { symbol, network: None } => {
                format!("crypto/{}", symbol.trim().to_uppercase())
            }
            Asset::Crypto { symbol, network: Some(net) } => {
                format!("crypto/{}/{}", symbol.trim().to_uppercase(), net.trim().to_lowercase())
            }
        };
        Self(id)
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
        assert_eq!(id_lower.as_str(), "currency/USD");
    }

    #[test]
    fn asset_id_is_human_readable_currency() {
        let asset = Asset::currency("USD");
        let id = AssetId::from_asset(&asset);
        assert_eq!(id.as_str(), "currency/USD");
    }

    #[test]
    fn asset_id_is_human_readable_equity() {
        let asset = Asset::equity("AAPL");
        let id = AssetId::from_asset(&asset);
        assert_eq!(id.as_str(), "equity/AAPL");
    }

    #[test]
    fn asset_id_is_human_readable_equity_with_exchange() {
        let asset = Asset::Equity {
            ticker: "AAPL".to_string(),
            exchange: Some("NYSE".to_string()),
        };
        let id = AssetId::from_asset(&asset);
        assert_eq!(id.as_str(), "equity/AAPL/NYSE");
    }

    #[test]
    fn asset_id_is_human_readable_crypto() {
        let asset = Asset::crypto("BTC");
        let id = AssetId::from_asset(&asset);
        assert_eq!(id.as_str(), "crypto/BTC");
    }

    #[test]
    fn asset_id_is_human_readable_crypto_with_network() {
        let asset = Asset::Crypto {
            symbol: "ETH".to_string(),
            network: Some("arbitrum".to_string()),
        };
        let id = AssetId::from_asset(&asset);
        assert_eq!(id.as_str(), "crypto/ETH/arbitrum");
    }
}

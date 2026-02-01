use serde::{Deserialize, Serialize};

/// Represents an asset type with type-specific identification fields.
/// The `asset_type` field determines what other fields are present.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Asset {
    Currency {
        iso_code: String,
    },
    Equity {
        ticker: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        exchange: Option<String>,
    },
    Crypto {
        symbol: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        network: Option<String>,
    },
}

impl Asset {
    pub fn currency(iso_code: impl Into<String>) -> Self {
        Asset::Currency {
            iso_code: iso_code.into(),
        }
    }

    pub fn crypto(symbol: impl Into<String>) -> Self {
        Asset::Crypto {
            symbol: symbol.into(),
            network: None,
        }
    }

    pub fn equity(ticker: impl Into<String>) -> Self {
        Asset::Equity {
            ticker: ticker.into(),
            exchange: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_serialization() {
        let usd = Asset::currency("USD");
        let json = serde_json::to_string(&usd).unwrap();
        assert_eq!(json, r#"{"type":"currency","iso_code":"USD"}"#);

        let btc = Asset::crypto("BTC");
        let json = serde_json::to_string(&btc).unwrap();
        assert_eq!(json, r#"{"type":"crypto","symbol":"BTC"}"#);
    }

    #[test]
    fn test_asset_equality() {
        let usd1 = Asset::currency("USD");
        let usd2 = Asset::currency("USD");
        assert_eq!(usd1, usd2);
    }
}

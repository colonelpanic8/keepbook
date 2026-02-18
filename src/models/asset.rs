use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

/// Represents an asset type with type-specific identification fields.
/// The `asset_type` field determines what other fields are present.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let iso_code = iso_code.into();
        Asset::Currency {
            iso_code: iso_code.trim().to_string(),
        }
    }

    pub fn crypto(symbol: impl Into<String>) -> Self {
        let symbol = symbol.into();
        Asset::Crypto {
            symbol: symbol.trim().to_string(),
            network: None,
        }
    }

    pub fn equity(ticker: impl Into<String>) -> Self {
        let ticker = ticker.into();
        Asset::Equity {
            ticker: ticker.trim().to_string(),
            exchange: None,
        }
    }

    pub fn normalized(&self) -> Self {
        match self {
            Asset::Currency { iso_code } => Asset::Currency {
                iso_code: normalize_currency_code(iso_code),
            },
            Asset::Equity { ticker, exchange } => Asset::Equity {
                ticker: normalize_upper(ticker),
                exchange: normalize_opt_upper(exchange),
            },
            Asset::Crypto { symbol, network } => Asset::Crypto {
                symbol: normalize_upper(symbol),
                network: normalize_opt_lower(network),
            },
        }
    }
}

fn normalize_currency_code(value: &str) -> String {
    let trimmed = value.trim();
    // Some sources provide ISO 4217 numeric codes (e.g. "840" for USD).
    // Normalize those into alpha codes where we can.
    match trimmed {
        "840" => "USD".to_string(),
        _ => trimmed.to_uppercase(),
    }
}

fn normalize_upper(value: &str) -> String {
    value.trim().to_uppercase()
}

fn normalize_opt_upper(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_uppercase())
}

fn normalize_opt_lower(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_lowercase())
}

impl PartialEq for Asset {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Asset::Currency { iso_code: a }, Asset::Currency { iso_code: b }) => {
                normalize_currency_code(a) == normalize_currency_code(b)
            }
            (
                Asset::Equity {
                    ticker: a,
                    exchange: ex_a,
                },
                Asset::Equity {
                    ticker: b,
                    exchange: ex_b,
                },
            ) => {
                normalize_upper(a) == normalize_upper(b)
                    && normalize_opt_upper(ex_a) == normalize_opt_upper(ex_b)
            }
            (
                Asset::Crypto {
                    symbol: a,
                    network: net_a,
                },
                Asset::Crypto {
                    symbol: b,
                    network: net_b,
                },
            ) => {
                normalize_upper(a) == normalize_upper(b)
                    && normalize_opt_lower(net_a) == normalize_opt_lower(net_b)
            }
            _ => false,
        }
    }
}

impl Eq for Asset {}

impl Hash for Asset {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Asset::Currency { iso_code } => {
                "currency".hash(state);
                normalize_currency_code(iso_code).hash(state);
            }
            Asset::Equity { ticker, exchange } => {
                "equity".hash(state);
                normalize_upper(ticker).hash(state);
                normalize_opt_upper(exchange).hash(state);
            }
            Asset::Crypto { symbol, network } => {
                "crypto".hash(state);
                normalize_upper(symbol).hash(state);
                normalize_opt_lower(network).hash(state);
            }
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

    #[test]
    fn test_asset_equality_is_case_insensitive() {
        let usd_lower = Asset::currency("usd");
        let usd_upper = Asset::currency("USD");
        assert_eq!(usd_lower, usd_upper);

        let usd_numeric = Asset::currency("840");
        assert_eq!(usd_numeric, usd_upper);

        let equity_lower = Asset::equity("aapl");
        let equity_upper = Asset::equity("AAPL");
        assert_eq!(equity_lower, equity_upper);

        let crypto_lower = Asset::crypto("btc");
        let crypto_upper = Asset::crypto("BTC");
        assert_eq!(crypto_lower, crypto_upper);

        let with_exchange = Asset::Equity {
            ticker: "aapl".to_string(),
            exchange: Some("nasdaq".to_string()),
        };
        let with_exchange_upper = Asset::Equity {
            ticker: "AAPL".to_string(),
            exchange: Some("NASDAQ".to_string()),
        };
        assert_eq!(with_exchange, with_exchange_upper);

        let with_network = Asset::Crypto {
            symbol: "eth".to_string(),
            network: Some("Arbitrum".to_string()),
        };
        let with_network_lower = Asset::Crypto {
            symbol: "ETH".to_string(),
            network: Some("arbitrum".to_string()),
        };
        assert_eq!(with_network, with_network_lower);
    }

    #[test]
    fn test_asset_hash_is_case_insensitive() {
        use std::collections::HashSet;

        let mut assets = HashSet::new();
        assets.insert(Asset::currency("USD"));
        assets.insert(Asset::equity("AAPL"));
        assets.insert(Asset::Crypto {
            symbol: "ETH".to_string(),
            network: Some("Arbitrum".to_string()),
        });

        assert!(assets.contains(&Asset::currency(" usd ")));
        assert!(assets.contains(&Asset::equity("aapl")));
        assert!(assets.contains(&Asset::Crypto {
            symbol: "eth".to_string(),
            network: Some("arbitrum".to_string()),
        }));
    }

    #[test]
    fn test_asset_normalized_canonicalizes_fields() {
        let currency = Asset::Currency {
            iso_code: " usd ".to_string(),
        };
        match currency.normalized() {
            Asset::Currency { iso_code } => assert_eq!(iso_code, "USD"),
            _ => panic!("expected currency asset"),
        }

        let equity = Asset::Equity {
            ticker: " aapl ".to_string(),
            exchange: Some(" nasdaq ".to_string()),
        };
        match equity.normalized() {
            Asset::Equity { ticker, exchange } => {
                assert_eq!(ticker, "AAPL");
                assert_eq!(exchange, Some("NASDAQ".to_string()));
            }
            _ => panic!("expected equity asset"),
        }

        let crypto = Asset::Crypto {
            symbol: " eth ".to_string(),
            network: Some(" Arbitrum ".to_string()),
        };
        match crypto.normalized() {
            Asset::Crypto { symbol, network } => {
                assert_eq!(symbol, "ETH");
                assert_eq!(network, Some("arbitrum".to_string()));
            }
            _ => panic!("expected crypto asset"),
        }
    }
}

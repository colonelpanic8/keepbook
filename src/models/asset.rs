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
    ManualValue {
        name: String,
        currency: String,
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

    pub fn manual_value(name: impl Into<String>, currency: impl Into<String>) -> Self {
        let name = name.into();
        let currency = currency.into();
        Asset::ManualValue {
            name: name.trim().to_string(),
            currency: currency.trim().to_string(),
        }
    }

    pub fn normalized(&self) -> Self {
        match self {
            Asset::Currency { iso_code } => Asset::Currency {
                iso_code: normalize_currency_code(iso_code),
            },
            Asset::ManualValue { name, currency } => Asset::ManualValue {
                name: normalize_name(name),
                currency: normalize_currency_code(currency),
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

fn normalize_name(value: &str) -> String {
    value.trim().to_string()
}

fn normalize_name_for_key(value: &str) -> String {
    value.trim().to_lowercase()
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
                Asset::ManualValue {
                    name: name_a,
                    currency: currency_a,
                },
                Asset::ManualValue {
                    name: name_b,
                    currency: currency_b,
                },
            ) => {
                normalize_name_for_key(name_a) == normalize_name_for_key(name_b)
                    && normalize_currency_code(currency_a) == normalize_currency_code(currency_b)
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
            Asset::ManualValue { name, currency } => {
                "manual_value".hash(state);
                normalize_name_for_key(name).hash(state);
                normalize_currency_code(currency).hash(state);
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
#[path = "../../tests/unit/models/asset_tests.rs"]
mod asset_tests;

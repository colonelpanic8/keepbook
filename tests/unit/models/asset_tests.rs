use super::*;

#[test]
fn test_asset_serialization() {
    let usd = Asset::currency("USD");
    let json = serde_json::to_string(&usd).unwrap();
    assert_eq!(json, r#"{"type":"currency","iso_code":"USD"}"#);

    let btc = Asset::crypto("BTC");
    let json = serde_json::to_string(&btc).unwrap();
    assert_eq!(json, r#"{"type":"crypto","symbol":"BTC"}"#);

    let value = Asset::manual_value("Expected Housing Value", "USD");
    let json = serde_json::to_string(&value).unwrap();
    assert_eq!(
        json,
        r#"{"type":"manual_value","name":"Expected Housing Value","currency":"USD"}"#
    );
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

    let manual_value_lower = Asset::manual_value("expected housing value", "usd");
    let manual_value_upper = Asset::manual_value("Expected Housing Value", "USD");
    assert_eq!(manual_value_lower, manual_value_upper);

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
    assets.insert(Asset::manual_value("Expected Housing Value", "USD"));

    assert!(assets.contains(&Asset::currency(" usd ")));
    assert!(assets.contains(&Asset::equity("aapl")));
    assert!(assets.contains(&Asset::Crypto {
        symbol: "eth".to_string(),
        network: Some("arbitrum".to_string()),
    }));
    assert!(assets.contains(&Asset::manual_value("expected housing value", "usd")));
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

    let value = Asset::ManualValue {
        name: " Expected Housing Value ".to_string(),
        currency: " usd ".to_string(),
    };
    match value.normalized() {
        Asset::ManualValue { name, currency } => {
            assert_eq!(name, "Expected Housing Value");
            assert_eq!(currency, "USD");
        }
        _ => panic!("expected manual value asset"),
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

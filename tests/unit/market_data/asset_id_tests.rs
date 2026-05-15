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
fn asset_id_is_human_readable_manual_value() {
    let asset = Asset::manual_value("Expected Housing Value", "USD");
    let id = AssetId::from_asset(&asset);
    assert_eq!(id.as_str(), "manual_value/USD/expected housing value");
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

#[test]
fn asset_id_ignores_empty_exchange_and_network() {
    let equity = Asset::Equity {
        ticker: "AAPL".to_string(),
        exchange: Some("".to_string()),
    };
    let equity_id = AssetId::from_asset(&equity);
    assert_eq!(equity_id.as_str(), "equity/AAPL");

    let crypto = Asset::Crypto {
        symbol: "BTC".to_string(),
        network: Some("   ".to_string()),
    };
    let crypto_id = AssetId::from_asset(&crypto);
    assert_eq!(crypto_id.as_str(), "crypto/BTC");
}

#[test]
fn asset_id_sanitizes_path_segments() {
    let equity = Asset::equity("BRK/B");
    let id = AssetId::from_asset(&equity);
    assert_eq!(id.as_str(), "equity/BRK-B");

    let equity_exchange = Asset::Equity {
        ticker: "BRK/B".to_string(),
        exchange: Some("X/NY".to_string()),
    };
    let id = AssetId::from_asset(&equity_exchange);
    assert_eq!(id.as_str(), "equity/BRK-B/X-NY");

    let crypto = Asset::Crypto {
        symbol: "ETH/ARB".to_string(),
        network: Some("Arb/One".to_string()),
    };
    let id = AssetId::from_asset(&crypto);
    assert_eq!(id.as_str(), "crypto/ETH-ARB/arb-one");
}

use super::*;

#[test]
fn test_format_symbol_no_exchange() {
    assert_eq!(MarketstackPriceSource::format_symbol("AAPL", None), "AAPL");
    assert_eq!(MarketstackPriceSource::format_symbol("aapl", None), "AAPL");
}

#[test]
fn test_format_symbol_us_exchanges() {
    assert_eq!(
        MarketstackPriceSource::format_symbol("AAPL", Some("NASDAQ")),
        "AAPL"
    );
    assert_eq!(
        MarketstackPriceSource::format_symbol("AAPL", Some("XNAS")),
        "AAPL"
    );
    assert_eq!(
        MarketstackPriceSource::format_symbol("IBM", Some("NYSE")),
        "IBM"
    );
    assert_eq!(
        MarketstackPriceSource::format_symbol("IBM", Some("XNYS")),
        "IBM"
    );
}

#[test]
fn test_format_symbol_international_exchanges() {
    assert_eq!(
        MarketstackPriceSource::format_symbol("VOD", Some("XLON")),
        "VOD.XLON"
    );
    assert_eq!(
        MarketstackPriceSource::format_symbol("VOD", Some("LSE")),
        "VOD.XLON"
    );
    assert_eq!(
        MarketstackPriceSource::format_symbol("SAP", Some("XFRA")),
        "SAP.XFRA"
    );
}

#[test]
fn test_parse_date() {
    let date = MarketstackPriceSource::parse_date("2024-01-15T00:00:00+0000").unwrap();
    assert_eq!(date, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());

    let date = MarketstackPriceSource::parse_date("2024-01-15").unwrap();
    assert_eq!(date, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
}

#[test]
fn test_parse_eod_response() {
    let json = r#"{
        "pagination": {
            "limit": 100,
            "offset": 0,
            "count": 1,
            "total": 1
        },
        "data": [
            {
                "open": 150.25,
                "high": 152.50,
                "low": 149.75,
                "close": 151.30,
                "volume": 45678900,
                "adj_high": 152.50,
                "adj_low": 149.75,
                "adj_close": 151.30,
                "adj_open": 150.25,
                "adj_volume": 45678900,
                "split_factor": 1.0,
                "dividend": 0.0,
                "symbol": "AAPL",
                "exchange": "XNAS",
                "date": "2024-01-15T00:00:00+0000"
            }
        ]
    }"#;

    let response: EodResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.data.len(), 1);
    assert_eq!(response.data[0].close, 151.30);
    assert_eq!(response.data[0].symbol, "AAPL");
    assert_eq!(response.data[0].date, "2024-01-15T00:00:00+0000");
}

#[test]
fn test_parse_empty_response() {
    let json = r#"{
        "pagination": {
            "limit": 100,
            "offset": 0,
            "count": 0,
            "total": 0
        },
        "data": []
    }"#;

    let response: EodResponse = serde_json::from_str(json).unwrap();
    assert!(response.data.is_empty());
}

#[test]
fn test_provider_name() {
    let provider = MarketstackPriceSource::new("test_key");
    assert_eq!(provider.name(), "marketstack");
}

#[tokio::test]
async fn test_non_equity_asset_returns_none() {
    let provider = MarketstackPriceSource::new("test_key");
    let asset = Asset::Crypto {
        symbol: "BTC".to_string(),
        network: None,
    };
    let asset_id = AssetId::from_asset(&asset);
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    let result = provider.fetch_close(&asset, &asset_id, date).await.unwrap();
    assert!(result.is_none());
}

#[test]
fn test_exchange_code_mapping() {
    // US exchanges return empty string (no suffix needed)
    assert_eq!(MarketstackPriceSource::map_exchange_code("NASDAQ"), "");
    assert_eq!(MarketstackPriceSource::map_exchange_code("NYSE"), "");
    assert_eq!(MarketstackPriceSource::map_exchange_code("XNAS"), "");
    assert_eq!(MarketstackPriceSource::map_exchange_code("XNYS"), "");

    // International exchanges return MIC codes
    assert_eq!(MarketstackPriceSource::map_exchange_code("LSE"), "XLON");
    assert_eq!(MarketstackPriceSource::map_exchange_code("XLON"), "XLON");
    assert_eq!(MarketstackPriceSource::map_exchange_code("TSX"), "XTSE");
    assert_eq!(MarketstackPriceSource::map_exchange_code("ASX"), "XASX");
}

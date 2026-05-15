use super::*;

const SAMPLE_RESPONSE: &str = r#"{
    "meta": {
        "symbol": "AAPL",
        "interval": "1day",
        "currency": "USD",
        "exchange_timezone": "America/New_York",
        "exchange": "NASDAQ",
        "mic_code": "XNAS",
        "type": "Common Stock"
    },
    "values": [
        {
            "datetime": "2024-01-15",
            "open": "186.06",
            "high": "187.00",
            "low": "183.62",
            "close": "185.92",
            "volume": "65076700"
        },
        {
            "datetime": "2024-01-12",
            "open": "186.06",
            "high": "186.74",
            "low": "185.19",
            "close": "185.59",
            "volume": "40477600"
        },
        {
            "datetime": "2024-01-11",
            "open": "186.54",
            "high": "187.05",
            "low": "185.15",
            "close": "185.18",
            "volume": "49128400"
        }
    ],
    "status": "ok"
}"#;

const SAMPLE_ERROR_RESPONSE: &str = r#"{
    "status": "error",
    "code": 400,
    "message": "No data is available for this query"
}"#;

const SAMPLE_RESPONSE_NO_CURRENCY: &str = r#"{
    "meta": {
        "symbol": "AAPL",
        "interval": "1day",
        "exchange_timezone": "America/New_York",
        "exchange": "NASDAQ",
        "type": "Common Stock"
    },
    "values": [
        {
            "datetime": "2024-01-15",
            "open": "186.06",
            "high": "187.00",
            "low": "183.62",
            "close": "185.92",
            "volume": "65076700"
        }
    ],
    "status": "ok"
}"#;

#[test]
fn test_parse_time_series_response() {
    let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE).unwrap();

    assert_eq!(response.meta.symbol, "AAPL");
    assert_eq!(response.meta.currency, Some("USD".to_string()));
    assert_eq!(response.meta.exchange, Some("NASDAQ".to_string()));
    assert_eq!(response.values.len(), 3);

    let first_value = &response.values[0];
    assert_eq!(first_value.datetime, "2024-01-15");
    assert_eq!(first_value.close, "185.92");
}

#[test]
fn test_parse_error_response() {
    let response: ErrorResponse = serde_json::from_str(SAMPLE_ERROR_RESPONSE).unwrap();

    assert_eq!(response.status, "error");
    assert_eq!(response.code, Some(400));
    assert!(response.message.contains("No data"));
}

#[test]
fn test_parse_response_no_currency() {
    let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE_NO_CURRENCY).unwrap();

    assert_eq!(response.meta.symbol, "AAPL");
    assert!(response.meta.currency.is_none());
}

#[test]
fn test_build_symbol_us_equity() {
    let symbol = TwelveDataPriceSource::build_symbol("aapl", None);
    assert_eq!(symbol, "AAPL");
}

#[test]
fn test_build_symbol_with_exchange() {
    let symbol = TwelveDataPriceSource::build_symbol("aapl", Some("nasdaq"));
    assert_eq!(symbol, "AAPL:NASDAQ");
}

#[test]
fn test_build_symbol_international() {
    let symbol = TwelveDataPriceSource::build_symbol("VOD", Some("LSE"));
    assert_eq!(symbol, "VOD:LSE");
}

#[test]
fn test_find_exact_date_in_values() {
    let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE).unwrap();
    let target_date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let target_str = target_date.format("%Y-%m-%d").to_string();

    let value = response.values.iter().find(|v| v.datetime == target_str);

    assert!(value.is_some());
    assert_eq!(value.unwrap().close, "185.92");
}

#[test]
fn test_find_previous_date_in_values() {
    let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE).unwrap();
    // Saturday - market closed, should find Friday's data
    let target_date = NaiveDate::from_ymd_opt(2024, 1, 13).unwrap();
    let target_str = target_date.format("%Y-%m-%d").to_string();

    // First try exact match
    let exact = response.values.iter().find(|v| v.datetime == target_str);
    assert!(exact.is_none());

    // Then find most recent before target
    let previous = response.values.iter().find(|v| {
        NaiveDate::parse_from_str(&v.datetime, "%Y-%m-%d")
            .map(|d| d <= target_date)
            .unwrap_or(false)
    });

    assert!(previous.is_some());
    // Should find 2024-01-12 (Friday)
    assert_eq!(previous.unwrap().datetime, "2024-01-12");
    assert_eq!(previous.unwrap().close, "185.59");
}

#[test]
fn test_provider_name() {
    let provider = TwelveDataPriceSource::new("test_key");
    assert_eq!(provider.name(), "twelve_data");
}

#[tokio::test]
async fn test_non_equity_asset_returns_none() {
    let provider = TwelveDataPriceSource::new("test_key");
    let asset = Asset::crypto("BTC");
    let asset_id = AssetId::from_asset(&asset);
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    let result = provider.fetch_close(&asset, &asset_id, date).await;

    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

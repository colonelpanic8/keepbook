use super::*;

const SAMPLE_RESPONSE: &str = r#"{
    "Meta Data": {
        "1. Information": "Daily Prices (open, high, low, close) and Volumes",
        "2. Symbol": "AAPL",
        "3. Last Refreshed": "2024-01-15",
        "4. Output Size": "Compact",
        "5. Time Zone": "US/Eastern"
    },
    "Time Series (Daily)": {
        "2024-01-15": {
            "1. open": "186.0600",
            "2. high": "187.4700",
            "3. low": "183.6200",
            "4. close": "185.9200",
            "5. volume": "65076672"
        },
        "2024-01-12": {
            "1. open": "186.0900",
            "2. high": "186.7400",
            "3. low": "185.1900",
            "4. close": "185.5900",
            "5. volume": "40477783"
        }
    }
}"#;

const ERROR_RESPONSE_RATE_LIMIT: &str = r#"{
    "Note": "Thank you for using Alpha Vantage! Our standard API rate limit is 25 requests per day."
}"#;

const ERROR_RESPONSE_INVALID_KEY: &str = r#"{
    "Error Message": "Invalid API call. Please retry or visit the documentation."
}"#;

const INFO_RESPONSE: &str = r#"{
    "Information": "Please consider upgrading to our premium service for more API calls."
}"#;

#[test]
fn test_parse_time_series_response() {
    let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE).unwrap();

    assert_eq!(response.meta_data.symbol, "AAPL");
    assert_eq!(response.time_series.len(), 2);

    let jan_15 = response.time_series.get("2024-01-15").unwrap();
    assert_eq!(jan_15.close, "185.9200");
    assert_eq!(jan_15.open, "186.0600");
    assert_eq!(jan_15.high, "187.4700");
    assert_eq!(jan_15.low, "183.6200");
    assert_eq!(jan_15.volume, "65076672");
}

#[test]
fn test_parse_price_point() {
    let provider = AlphaVantagePriceSource::new("test_key");
    let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE).unwrap();
    let asset_id = AssetId::from("test_asset_id");
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    let price_point = provider.parse_response(&response, &asset_id, date).unwrap();

    assert_eq!(price_point.price, "185.9200");
    assert_eq!(price_point.quote_currency, "USD");
    assert_eq!(price_point.kind, PriceKind::Close);
    assert_eq!(price_point.source, "alpha_vantage");
    assert_eq!(price_point.as_of_date, date);
}

#[test]
fn test_parse_price_point_missing_date() {
    let provider = AlphaVantagePriceSource::new("test_key");
    let response: TimeSeriesResponse = serde_json::from_str(SAMPLE_RESPONSE).unwrap();
    let asset_id = AssetId::from("test_asset_id");
    let date = NaiveDate::from_ymd_opt(2024, 1, 14).unwrap(); // Not in response

    let price_point = provider.parse_response(&response, &asset_id, date);

    assert!(price_point.is_none());
}

#[test]
fn test_parse_error_response_rate_limit() {
    let error: ErrorResponse = serde_json::from_str(ERROR_RESPONSE_RATE_LIMIT).unwrap();
    assert!(error.note.is_some());
    assert!(error.note.unwrap().contains("25 requests per day"));
}

#[test]
fn test_parse_error_response_invalid_key() {
    let error: ErrorResponse = serde_json::from_str(ERROR_RESPONSE_INVALID_KEY).unwrap();
    assert!(error.error_message.is_some());
    assert!(error.error_message.unwrap().contains("Invalid API call"));
}

#[test]
fn test_parse_info_response() {
    let error: ErrorResponse = serde_json::from_str(INFO_RESPONSE).unwrap();
    assert!(error.information.is_some());
}

#[test]
fn test_format_symbol_us_no_exchange() {
    let provider = AlphaVantagePriceSource::new("test_key");
    assert_eq!(provider.format_symbol("aapl", None), "AAPL");
    assert_eq!(provider.format_symbol("MSFT", None), "MSFT");
}

#[test]
fn test_format_symbol_us_exchanges() {
    let provider = AlphaVantagePriceSource::new("test_key");
    assert_eq!(provider.format_symbol("aapl", Some("NYSE")), "AAPL");
    assert_eq!(provider.format_symbol("msft", Some("NASDAQ")), "MSFT");
    assert_eq!(provider.format_symbol("goog", Some("XNAS")), "GOOG");
    assert_eq!(provider.format_symbol("jpm", Some("XNYS")), "JPM");
}

#[test]
fn test_format_symbol_international_exchanges() {
    let provider = AlphaVantagePriceSource::new("test_key");
    assert_eq!(provider.format_symbol("BMW", Some("XETR")), "BMW.DEX");
    assert_eq!(provider.format_symbol("BARC", Some("XLON")), "BARC.LON");
    assert_eq!(provider.format_symbol("RY", Some("XTSE")), "RY.TRT");
    assert_eq!(provider.format_symbol("7203", Some("XTKS")), "7203.TYO");
    assert_eq!(provider.format_symbol("BHP", Some("XASX")), "BHP.AX");
    assert_eq!(provider.format_symbol("OR", Some("XPAR")), "OR.PAR");
}

#[test]
fn test_format_symbol_unknown_exchange() {
    let provider = AlphaVantagePriceSource::new("test_key");
    assert_eq!(
        provider.format_symbol("ticker", Some("UNKNOWN")),
        "TICKER.UNKNOWN"
    );
}

#[test]
fn test_provider_name() {
    let provider = AlphaVantagePriceSource::new("test_key");
    assert_eq!(provider.name(), "alpha_vantage");
}

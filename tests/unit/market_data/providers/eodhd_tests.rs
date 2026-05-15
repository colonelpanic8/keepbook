use super::*;

const SAMPLE_EODHD_RESPONSE: &str = r#"[
    {
        "date": "2024-01-15",
        "open": 185.05,
        "high": 186.22,
        "low": 184.82,
        "close": 186.01,
        "adjusted_close": 185.45,
        "volume": 52894000
    }
]"#;

const SAMPLE_EODHD_RESPONSE_EMPTY: &str = "[]";

const SAMPLE_EODHD_RESPONSE_NO_CLOSE: &str = r#"[
    {
        "date": "2024-01-15",
        "open": 185.05,
        "high": 186.22,
        "low": 184.82,
        "close": null,
        "adjusted_close": null,
        "volume": 52894000
    }
]"#;

#[test]
fn test_parse_eodhd_response() {
    let data: Vec<EodhdEodResponse> = serde_json::from_str(SAMPLE_EODHD_RESPONSE).unwrap();
    assert_eq!(data.len(), 1);

    let entry = &data[0];
    assert_eq!(entry.date, "2024-01-15");
    assert_eq!(entry.close, Some(186.01));
    assert_eq!(entry.volume, Some(52894000));
}

#[test]
fn test_parse_empty_response() {
    let data: Vec<EodhdEodResponse> = serde_json::from_str(SAMPLE_EODHD_RESPONSE_EMPTY).unwrap();
    assert!(data.is_empty());
}

#[test]
fn test_parse_response_no_close() {
    let data: Vec<EodhdEodResponse> = serde_json::from_str(SAMPLE_EODHD_RESPONSE_NO_CLOSE).unwrap();
    assert_eq!(data.len(), 1);

    let asset = Asset::equity("AAPL");
    let asset_id = AssetId::from_asset(&asset);

    let result = EodhdPriceSource::parse_response(&data[0], &asset_id, None).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_parse_response_to_price_point() {
    let data: Vec<EodhdEodResponse> = serde_json::from_str(SAMPLE_EODHD_RESPONSE).unwrap();

    let asset = Asset::equity("AAPL");
    let asset_id = AssetId::from_asset(&asset);

    let price_point = EodhdPriceSource::parse_response(&data[0], &asset_id, None)
        .unwrap()
        .unwrap();

    assert_eq!(price_point.price, "186.01");
    assert_eq!(price_point.quote_currency, "USD");
    assert_eq!(price_point.kind, PriceKind::Close);
    assert_eq!(price_point.source, "eodhd");
    assert_eq!(
        price_point.as_of_date,
        NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
    );
}

#[test]
fn test_parse_response_with_exchange() {
    let data: Vec<EodhdEodResponse> = serde_json::from_str(SAMPLE_EODHD_RESPONSE).unwrap();

    let asset = Asset::Equity {
        ticker: "VOD".to_string(),
        exchange: Some("LSE".to_string()),
    };
    let asset_id = AssetId::from_asset(&asset);

    let price_point = EodhdPriceSource::parse_response(&data[0], &asset_id, Some("LSE"))
        .unwrap()
        .unwrap();

    assert_eq!(price_point.quote_currency, "GBP");
}

#[test]
fn test_build_symbol_us() {
    assert_eq!(EodhdPriceSource::build_symbol("AAPL", None), "AAPL.US");
    assert_eq!(
        EodhdPriceSource::build_symbol("AAPL", Some("NYSE")),
        "AAPL.US"
    );
    assert_eq!(
        EodhdPriceSource::build_symbol("AAPL", Some("NASDAQ")),
        "AAPL.US"
    );
    assert_eq!(
        EodhdPriceSource::build_symbol("AAPL", Some("XNYS")),
        "AAPL.US"
    );
    assert_eq!(
        EodhdPriceSource::build_symbol("AAPL", Some("XNAS")),
        "AAPL.US"
    );
}

#[test]
fn test_build_symbol_uk() {
    assert_eq!(
        EodhdPriceSource::build_symbol("VOD", Some("LSE")),
        "VOD.LSE"
    );
    assert_eq!(
        EodhdPriceSource::build_symbol("VOD", Some("XLON")),
        "VOD.LSE"
    );
}

#[test]
fn test_build_symbol_germany() {
    assert_eq!(
        EodhdPriceSource::build_symbol("SAP", Some("XETRA")),
        "SAP.XETRA"
    );
    assert_eq!(
        EodhdPriceSource::build_symbol("SAP", Some("XETR")),
        "SAP.XETRA"
    );
    assert_eq!(
        EodhdPriceSource::build_symbol("SAP", Some("FRANKFURT")),
        "SAP.F"
    );
}

#[test]
fn test_build_symbol_case_insensitive() {
    assert_eq!(EodhdPriceSource::build_symbol("aapl", None), "AAPL.US");
    assert_eq!(
        EodhdPriceSource::build_symbol("Aapl", Some("nyse")),
        "AAPL.US"
    );
}

#[test]
fn test_quote_currency_mapping() {
    assert_eq!(EodhdPriceSource::quote_currency_for_exchange(None), "USD");
    assert_eq!(
        EodhdPriceSource::quote_currency_for_exchange(Some("NYSE")),
        "USD"
    );
    assert_eq!(
        EodhdPriceSource::quote_currency_for_exchange(Some("LSE")),
        "GBP"
    );
    assert_eq!(
        EodhdPriceSource::quote_currency_for_exchange(Some("XETRA")),
        "EUR"
    );
    assert_eq!(
        EodhdPriceSource::quote_currency_for_exchange(Some("TSE")),
        "JPY"
    );
    assert_eq!(
        EodhdPriceSource::quote_currency_for_exchange(Some("HKEX")),
        "HKD"
    );
    assert_eq!(
        EodhdPriceSource::quote_currency_for_exchange(Some("ASX")),
        "AUD"
    );
    assert_eq!(
        EodhdPriceSource::quote_currency_for_exchange(Some("TSX")),
        "CAD"
    );
    assert_eq!(
        EodhdPriceSource::quote_currency_for_exchange(Some("SIX")),
        "CHF"
    );
    assert_eq!(
        EodhdPriceSource::quote_currency_for_exchange(Some("SGX")),
        "SGD"
    );
    assert_eq!(
        EodhdPriceSource::quote_currency_for_exchange(Some("BSE")),
        "INR"
    );
}

#[test]
fn test_exchange_mapping() {
    // US exchanges
    assert_eq!(EodhdPriceSource::map_exchange(Some("NYSE")), "US");
    assert_eq!(EodhdPriceSource::map_exchange(Some("NASDAQ")), "US");
    assert_eq!(EodhdPriceSource::map_exchange(Some("XNYS")), "US");
    assert_eq!(EodhdPriceSource::map_exchange(Some("XNAS")), "US");

    // International
    assert_eq!(EodhdPriceSource::map_exchange(Some("LSE")), "LSE");
    assert_eq!(EodhdPriceSource::map_exchange(Some("XLON")), "LSE");
    assert_eq!(EodhdPriceSource::map_exchange(Some("XETRA")), "XETRA");
    assert_eq!(EodhdPriceSource::map_exchange(Some("TSE")), "TSE");

    // Default
    assert_eq!(EodhdPriceSource::map_exchange(None), "US");
    assert_eq!(EodhdPriceSource::map_exchange(Some("UNKNOWN")), "US");
}

#[test]
fn test_provider_name() {
    let provider = EodhdPriceSource::new("test_key");
    assert_eq!(provider.name(), "eodhd");
}

// Multi-day response parsing test
const SAMPLE_EODHD_MULTI_DAY: &str = r#"[
    {
        "date": "2024-01-15",
        "open": 185.05,
        "high": 186.22,
        "low": 184.82,
        "close": 186.01,
        "adjusted_close": 185.45,
        "volume": 52894000
    },
    {
        "date": "2024-01-16",
        "open": 186.50,
        "high": 187.00,
        "low": 185.20,
        "close": 185.75,
        "adjusted_close": 185.19,
        "volume": 48750000
    }
]"#;

#[test]
fn test_parse_multi_day_response() {
    let data: Vec<EodhdEodResponse> = serde_json::from_str(SAMPLE_EODHD_MULTI_DAY).unwrap();
    assert_eq!(data.len(), 2);

    assert_eq!(data[0].date, "2024-01-15");
    assert_eq!(data[0].close, Some(186.01));

    assert_eq!(data[1].date, "2024-01-16");
    assert_eq!(data[1].close, Some(185.75));
}

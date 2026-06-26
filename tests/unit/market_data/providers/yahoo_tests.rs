use super::*;

// Real-shape Yahoo v8/finance/chart response (trimmed). Note `close` is the
// split-adjusted series: AAPL 2020-01-02 = 75.0875 == raw 300.35 / the later 4:1 split.
const SAMPLE_RESPONSE: &str = r#"{
  "chart": {
    "result": [
      {
        "meta": {
          "currency": "USD",
          "symbol": "AAPL",
          "gmtoffset": -14400,
          "regularMarketPrice": 275.15,
          "regularMarketTime": 1750876200
        },
        "timestamp": [1577975400, 1578061800, 1578321000],
        "indicators": {
          "quote": [
            { "close": [75.0875015258789, 74.35749816894531, null] }
          ]
        }
      }
    ],
    "error": null
  }
}"#;

const SAMPLE_NO_DATA: &str = r#"{
  "chart": {
    "result": null,
    "error": { "code": "Not Found", "description": "No data found, symbol may be delisted" }
  }
}"#;

#[test]
fn test_parse_chart_body_ok() {
    let result = parse_chart_body(SAMPLE_RESPONSE).unwrap().expect("result present");
    assert_eq!(result.meta.currency, Some("USD".to_string()));
    assert_eq!(result.meta.gmtoffset, -14400);
    assert_eq!(result.timestamp.as_ref().unwrap().len(), 3);
}

#[test]
fn test_parse_chart_body_no_data_is_none() {
    let result = parse_chart_body(SAMPLE_NO_DATA).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_extract_points_dates_prices_and_skips_nulls() {
    let result = parse_chart_body(SAMPLE_RESPONSE).unwrap().unwrap();
    let points = extract_points(&result);
    // The third bar has a null close and must be skipped.
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].0, NaiveDate::from_ymd_opt(2020, 1, 2).unwrap());
    // Split-adjusted close with f64 noise stripped to a clean decimal.
    assert_eq!(points[0].1, "75.0875");
    assert_eq!(points[1].0, NaiveDate::from_ymd_opt(2020, 1, 3).unwrap());
    assert_eq!(points[1].1, "74.3575");
}

#[test]
fn test_decimal_price_strips_float_noise() {
    assert_eq!(decimal_price(275.1499938964844).unwrap(), "275.15");
    assert_eq!(decimal_price(680.8200073242188).unwrap(), "680.82");
}

#[test]
fn test_build_symbol_us_equity() {
    assert_eq!(YahooPriceSource::build_symbol("aapl", None), "AAPL");
}

#[test]
fn test_build_symbol_with_exchange_suffix() {
    assert_eq!(YahooPriceSource::build_symbol("ora", Some("pa")), "ORA.PA");
}

#[test]
fn test_provider_name() {
    assert_eq!(YahooPriceSource::new().name(), "yahoo");
}

#[tokio::test]
async fn test_non_equity_asset_returns_none() {
    let provider = YahooPriceSource::new();
    let asset = Asset::crypto("BTC");
    let asset_id = AssetId::from_asset(&asset);
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let result = provider.fetch_close(&asset, &asset_id, date).await.unwrap();
    assert!(result.is_none());
}

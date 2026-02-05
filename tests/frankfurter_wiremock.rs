use anyhow::Result;
use chrono::NaiveDate;
use keepbook::market_data::providers::frankfurter::FrankfurterRateSource;
use keepbook::market_data::FxRateSource;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn frankfurter_same_currency_skips_http() -> Result<()> {
    let server = MockServer::start().await;
    let provider = FrankfurterRateSource::new().with_base_url(server.uri());

    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let result = provider.fetch_close("USD", "USD", date).await?;

    let fx = result.expect("expected FX rate");
    assert_eq!(fx.rate, "1");

    let requests = server.received_requests().await.unwrap_or_default();
    assert!(requests.is_empty(), "expected no HTTP requests");

    Ok(())
}

#[tokio::test]
async fn frankfurter_cross_rate_uses_eur_rates() -> Result<()> {
    let server = MockServer::start().await;
    let provider = FrankfurterRateSource::new().with_base_url(server.uri());

    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

    let body = r#"{
        "amount": 1.0,
        "base": "EUR",
        "date": "2024-01-15",
        "rates": {
            "USD": 1.2,
            "GBP": 0.8
        }
    }"#;

    Mock::given(method("GET"))
        .and(path("/2024-01-15"))
        .and(query_param("from", "EUR"))
        .and(query_param("to", "USD,GBP"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(&server)
        .await;

    let result = provider.fetch_close("USD", "GBP", date).await?;
    let fx = result.expect("expected FX rate");

    let rate: f64 = fx.rate.parse().unwrap();
    let expected = 0.8 / 1.2;
    assert!((rate - expected).abs() < 1e-6);

    Ok(())
}

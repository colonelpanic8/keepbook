use std::sync::Arc;

use anyhow::Result;
use chrono::{Duration, TimeZone, Utc};
use keepbook::clock::{Clock, FixedClock};
use keepbook::market_data::providers::coingecko::CoinGeckoPriceSource;
use keepbook::market_data::{AssetId, CryptoPriceSource};
use keepbook::models::Asset;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixed_clock() -> Arc<FixedClock> {
    Arc::new(FixedClock::new(
        Utc.with_ymd_and_hms(2026, 2, 5, 12, 0, 0).unwrap(),
    ))
}

#[tokio::test]
async fn coingecko_fetch_close_hits_mock_server() -> Result<()> {
    let server = MockServer::start().await;
    let clock = fixed_clock();
    let provider = CoinGeckoPriceSource::new()
        .with_base_url(server.uri())
        .with_clock(clock.clone());

    let date = clock.today() - Duration::days(1);
    let date_str = date.format("%d-%m-%Y").to_string();

    let body = r#"
        {
            "id": "bitcoin",
            "symbol": "btc",
            "name": "Bitcoin",
            "market_data": {
                "current_price": {
                    "usd": 42000.0
                }
            }
        }"#
    .to_string();

    Mock::given(method("GET"))
        .and(path("/coins/bitcoin/history"))
        .and(query_param("date", date_str.as_str()))
        .and(query_param("localization", "false"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/json"))
        .mount(&server)
        .await;

    let asset = Asset::crypto("BTC");
    let asset_id = AssetId::from_asset(&asset);
    let price = provider.fetch_close(&asset, &asset_id, date).await?;

    let price = price.expect("expected price");
    assert_eq!(price.price, "42000");
    assert_eq!(price.quote_currency.to_uppercase(), "USD");

    Ok(())
}

#[tokio::test]
async fn coingecko_skips_too_old_dates_without_http() -> Result<()> {
    let server = MockServer::start().await;
    let clock = fixed_clock();
    let provider = CoinGeckoPriceSource::new()
        .with_base_url(server.uri())
        .with_clock(clock.clone());

    let date = clock.today() - Duration::days(366);
    let asset = Asset::crypto("BTC");
    let asset_id = AssetId::from_asset(&asset);
    let result = provider.fetch_close(&asset, &asset_id, date).await?;

    assert!(result.is_none(), "expected no price for old date");

    let requests = server.received_requests().await.unwrap_or_default();
    assert!(requests.is_empty(), "expected no HTTP requests");

    Ok(())
}

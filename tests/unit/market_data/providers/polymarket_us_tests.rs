use super::*;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn source(server: &MockServer) -> PolymarketUsPriceSource {
    PolymarketUsPriceSource::new().with_base_url(server.uri())
}

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn ts(value: &str) -> i64 {
    DateTime::parse_from_rfc3339(value).unwrap().timestamp()
}

#[tokio::test]
async fn fetch_quote_prefers_current_price_over_settlement() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/markets/settled-market/bbo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "marketData": {
                "currentPx": {"value": "0.6335", "currency": "USD"},
                "settlementPx": {"value": "0.1590", "currency": "USD"}
            }
        })))
        .mount(&server)
        .await;

    let asset = polymarket_us_asset("settled-market", ContractSide::Yes, None);
    let price = source(&server)
        .fetch_quote(&asset, &AssetId::from_asset(&asset))
        .await?
        .expect("quote");

    assert_eq!(price.price, "0.6335");
    assert_eq!(price.kind, PriceKind::Quote);
    assert_eq!(price.asset_id.as_str(), "polymarket_us/settled-market/yes");
    Ok(())
}

#[tokio::test]
async fn fetch_closes_prices_no_contracts_from_each_days_closing_midpoint() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/price-history"))
        .and(query_param("symbol", "long-market"))
        .and(query_param("fixedInterval", "INTERVAL_ALL"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "history": [
                {"timestamp": ts("2026-09-30T21:00:00Z"), "longPrice": 0.5, "shortPrice": 0.52},
                {"timestamp": ts("2026-10-01T00:00:00Z"), "longPrice": 0.55, "shortPrice": 0.47},
                {"timestamp": ts("2026-10-01T21:00:00Z"), "longPrice": 0.61, "shortPrice": 0.41},
                {"timestamp": ts("2026-10-02T00:00:00Z"), "longPrice": 0.64, "shortPrice": 0.38},
                {"timestamp": ts("2026-10-03T00:00:00Z"), "longPrice": 0.7},
                {"timestamp": ts("2026-10-04T00:00:00Z"), "longPrice": 0.8, "shortPrice": 0.22}
            ]
        })))
        .mount(&server)
        .await;

    let asset = polymarket_us_asset("long-market", ContractSide::No, None);
    let prices = source(&server)
        .fetch_closes(
            &asset,
            &AssetId::from_asset(&asset),
            date("2026-10-01"),
            date("2026-10-02"),
        )
        .await?;

    let closes: Vec<_> = prices
        .iter()
        .map(|p| (p.as_of_date, p.price.as_str()))
        .collect();
    assert_eq!(
        closes,
        vec![(date("2026-10-01"), "0.37"), (date("2026-10-02"), "0.3")]
    );
    Ok(())
}

use super::*;
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn source(server: &MockServer) -> KalshiPriceSource {
    KalshiPriceSource::new().with_base_url(server.uri())
}

fn date(value: &str) -> NaiveDate {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
}

fn candle(end: &str, bid: &str, ask: &str, trade: Option<&str>) -> serde_json::Value {
    let end_period_ts = DateTime::parse_from_rfc3339(end).unwrap().timestamp();
    json!({
        "end_period_ts": end_period_ts,
        "yes_bid": {"open_dollars": bid, "low_dollars": bid, "high_dollars": bid, "close_dollars": bid},
        "yes_ask": {"open_dollars": ask, "low_dollars": ask, "high_dollars": ask, "close_dollars": ask},
        "price": {"close_dollars": trade},
        "volume_fp": "0.00",
        "open_interest_fp": "0.00"
    })
}

#[tokio::test]
async fn fetch_quote_prices_the_held_side() -> Result<()> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/trade-api/v2/markets"))
        .and(query_param("tickers", "FED-DEC"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "markets": [{
                "ticker": "FED-DEC",
                "yes_bid_dollars": "0.7000",
                "yes_ask_dollars": "0.7200",
                "no_bid_dollars": "0.2800",
                "no_ask_dollars": "0.3000",
                "last_price_dollars": "0.7100",
                "notional_value_dollars": "1.0000"
            }],
            "cursor": ""
        })))
        .mount(&server)
        .await;

    let asset = kalshi_asset("FED-DEC", KalshiSide::No, None);
    let price = source(&server)
        .fetch_quote(&asset, &AssetId::from_asset(&asset))
        .await?
        .expect("quote");

    assert_eq!(price.price, "0.29");
    assert_eq!(price.kind, PriceKind::Quote);
    assert_eq!(price.asset_id.as_str(), "kalshi/FED-DEC/no");
    Ok(())
}

#[tokio::test]
async fn fetch_closes_maps_daily_candles_to_the_day_they_cover() -> Result<()> {
    let server = MockServer::start().await;
    // Daily candles close at Eastern midnight, 04:00 UTC during daylight time.
    let start_ts = DateTime::parse_from_rfc3339("2026-10-01T04:00:00Z")
        .unwrap()
        .timestamp();
    let end_ts = DateTime::parse_from_rfc3339("2026-10-04T04:00:00Z")
        .unwrap()
        .timestamp();
    Mock::given(method("GET"))
        .and(path("/trade-api/v2/markets/candlesticks"))
        .and(query_param("market_tickers", "FED-DEC"))
        .and(query_param("start_ts", start_ts.to_string()))
        .and(query_param("end_ts", end_ts.to_string()))
        .and(query_param("period_interval", "1440"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "markets": [{
                "market_ticker": "FED-DEC",
                "candlesticks": [
                    candle("2026-10-02T04:00:00Z", "0.6000", "0.6400", Some("0.6100")),
                    // One-sided book: falls back to the last trade.
                    candle("2026-10-03T04:00:00Z", "0.0000", "0.6600", Some("0.6500")),
                    // No quotes and no trades.
                    candle("2026-10-04T04:00:00Z", "0.0000", "1.0000", None),
                ]
            }]
        })))
        .mount(&server)
        .await;

    let asset = kalshi_asset("FED-DEC", KalshiSide::No, None);
    let prices = source(&server)
        .fetch_closes(
            &asset,
            &AssetId::from_asset(&asset),
            date("2026-10-01"),
            date("2026-10-03"),
        )
        .await?;

    let closes: Vec<_> = prices
        .iter()
        .map(|p| (p.as_of_date, p.price.as_str(), p.kind))
        .collect();
    assert_eq!(
        closes,
        vec![
            (date("2026-10-01"), "0.38", PriceKind::Close),
            (date("2026-10-02"), "0.35", PriceKind::Close),
        ]
    );
    Ok(())
}

#[test]
fn display_name_drops_a_no_subtitle_that_repeats_the_yes_side() {
    let market: KalshiMarket = serde_json::from_value(json!({
        "ticker": "NHL-OVER",
        "title": "1st Period: Over 5.5 goals scored",
        "yes_sub_title": "Over 5.5 goals",
        "no_sub_title": "Over 5.5 goals"
    }))
    .unwrap();

    assert_eq!(
        market.display_name(KalshiSide::Yes),
        "1st Period: Over 5.5 goals scored (Yes: Over 5.5 goals)"
    );
    assert_eq!(
        market.display_name(KalshiSide::No),
        "1st Period: Over 5.5 goals scored (No)"
    );
}

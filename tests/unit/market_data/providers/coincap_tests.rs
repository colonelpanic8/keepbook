use super::*;

#[test]
fn parse_history_response() {
    let json = r#"{
        "data": [
            {
                "priceUsd": "42685.1234",
                "time": 1704067200000
            }
        ],
        "timestamp": 1704153600000
    }"#;

    let response: HistoryResponse = serde_json::from_str(json).expect("parse history");
    assert_eq!(response.data.len(), 1);
    assert_eq!(response.data[0].price_usd.as_str().unwrap(), "42685.1234");
}

#[test]
fn parse_asset_search_response() {
    let json = r#"{
        "data": [
            { "id": "bitcoin", "symbol": "BTC" },
            { "id": "bitcash", "symbol": "BTC" }
        ],
        "timestamp": 1704153600000
    }"#;

    let response: AssetSearchResponse = serde_json::from_str(json).expect("parse assets");
    assert_eq!(response.data.len(), 2);
    assert_eq!(response.data[0].id, "bitcoin");
}

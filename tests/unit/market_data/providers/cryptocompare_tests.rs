use super::*;

#[test]
fn parse_histoday_response() {
    let json = r#"{
        "Response": "Success",
        "Data": {
            "Data": [
                { "time": 1704067200, "close": 42850.12 },
                { "time": 1704153600, "close": 43500.34 }
            ]
        }
    }"#;

    let response: HistoryResponse = serde_json::from_str(json).expect("parse");
    assert_eq!(response.response, "Success");
    let points = response.data.unwrap().data;
    assert_eq!(points.len(), 2);
    assert_eq!(points[1].close, Some(43500.34));
}

use super::*;

const WIRE: &str = r#"{
  "candidate_key": "v1|spotify|monthly|-11.99|{\"iso_code\":\"USD\",\"type\":\"currency\"}",
  "review_status": "proposed",
  "name": "Spotify.com",
  "normalized_name": "spotify",
  "status": "confirmed",
  "cadence": "monthly",
  "estimated_interval_days": "30.44",
  "estimated_recurring_cost": "11.99",
  "estimated_annual_cost": "143.88",
  "confidence": "0.92",
  "cadence_score": "0.97",
  "occurrence_count": 4,
  "first_seen": "2026-01-14",
  "last_seen": "2026-04-14",
  "next_expected": "2026-05-14",
  "amount": { "typical": "-11.99", "min": "-11.99", "max": "-11.99",
              "asset": { "type": "currency", "iso_code": "USD" } },
  "reason_codes": ["monthly_cadence"],
  "transactions": [{ "id": "tx-1", "account_id": "acct-1", "account_name": "Checking",
                     "date": "2026-01-14", "description": "SPOTIFY USA 1234", "amount": "-11.99" }]
}"#;

#[test]
fn output_round_trips_and_key_order_is_unchanged() {
    let parsed: ReviewedRecurringTransactionOutput = serde_json::from_str(WIRE).unwrap();
    assert_eq!(
        serde_json::to_string(&parsed).unwrap(),
        concat!(
            r##"{"candidate_key":"v1|spotify|monthly|-11.99|{\"iso_code\":\"USD\",\"type\":\"currency\"}","##,
            r#""review_status":"proposed","name":"Spotify.com","normalized_name":"spotify","#,
            r#""status":"confirmed","cadence":"monthly","estimated_interval_days":"30.44","#,
            r#""estimated_recurring_cost":"11.99","estimated_annual_cost":"143.88","#,
            r#""confidence":"0.92","cadence_score":"0.97","occurrence_count":4,"#,
            r#""first_seen":"2026-01-14","last_seen":"2026-04-14","next_expected":"2026-05-14","#,
            r#""amount":{"typical":"-11.99","min":"-11.99","max":"-11.99","#,
            r#""asset":{"iso_code":"USD","type":"currency"}},"reason_codes":["monthly_cadence"],"#,
            r#""transactions":[{"id":"tx-1","account_id":"acct-1","account_name":"Checking","#,
            r#""date":"2026-01-14","description":"SPOTIFY USA 1234","amount":"-11.99"}]}"#,
        )
    );
}

#[test]
fn next_expected_is_omitted_when_absent() {
    let mut value: serde_json::Value = serde_json::from_str(WIRE).unwrap();
    value.as_object_mut().unwrap().remove("next_expected");
    let parsed: ReviewedRecurringTransactionOutput = serde_json::from_value(value).unwrap();
    let out = serde_json::to_value(&parsed).unwrap();
    assert!(out.get("next_expected").is_none());
}

#[test]
fn review_input_still_accepts_status_plus_candidate() {
    let body = format!(r#"{{"status":"verified","candidate":{WIRE}}}"#);
    let input: RecurringTransactionReviewInput = serde_json::from_str(&body).unwrap();
    assert_eq!(input.status, "verified");
    assert_eq!(input.candidate.candidate.normalized_name, "spotify");
    assert_eq!(input.candidate.candidate.transactions.len(), 1);
}

#[test]
fn review_input_tolerates_missing_estimate_fields() {
    let mut candidate: serde_json::Value = serde_json::from_str(WIRE).unwrap();
    for field in [
        "estimated_interval_days",
        "estimated_recurring_cost",
        "estimated_annual_cost",
    ] {
        candidate.as_object_mut().unwrap().remove(field);
    }
    let body = serde_json::json!({"status": "dismissed", "candidate": candidate});
    let input: RecurringTransactionReviewInput = serde_json::from_value(body).unwrap();
    assert_eq!(input.candidate.candidate.estimated_annual_cost, "");
}

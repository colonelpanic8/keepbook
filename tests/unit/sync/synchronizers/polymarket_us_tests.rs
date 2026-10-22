use super::*;
use crate::models::ConnectionConfig;
use crate::storage::MemoryStorage;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::json;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

const SEED: [u8; 32] = [7; 32];

fn test_synchronizer(server: &MockServer) -> PolymarketUsSynchronizer {
    let signing_key = SigningKey::from_bytes(&SEED);
    // Issued secrets are the 64-byte seed + public key pair.
    let mut keypair = SEED.to_vec();
    keypair.extend_from_slice(signing_key.verifying_key().as_bytes());
    PolymarketUsSynchronizer::new(
        "test-key-id".to_string(),
        SecretString::new(STANDARD.encode(keypair).into()),
    )
    .with_base_url(server.uri())
}

fn test_connection() -> Connection {
    Connection::new(ConnectionConfig {
        name: "Polymarket US".to_string(),
        synchronizer: "polymarket_us".to_string(),
        credentials: None,
        balance_staleness: None,
    })
}

/// Accepts only requests signed over `timestamp + method + path` by the test key.
struct ValidSignature;

impl Match for ValidSignature {
    fn matches(&self, request: &Request) -> bool {
        let header = |name: &str| {
            request
                .headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let (Some(key_id), Some(timestamp), Some(signature)) = (
            header("X-PM-Access-Key"),
            header("X-PM-Timestamp"),
            header("X-PM-Signature"),
        ) else {
            return false;
        };
        let Some(signature) = STANDARD
            .decode(signature)
            .ok()
            .and_then(|bytes| Signature::from_slice(&bytes).ok())
        else {
            return false;
        };
        let message = format!("{timestamp}{}{}", request.method, request.url.path());
        let verifying_key: VerifyingKey = SigningKey::from_bytes(&SEED).verifying_key();
        key_id == "test-key-id" && verifying_key.verify(message.as_bytes(), &signature).is_ok()
    }
}

fn position(net: &str, cost: &str, expired: bool, title: &str, outcome: &str) -> serde_json::Value {
    json!({
        "netPositionDecimal": net,
        "cost": {"value": cost, "currency": "USD"},
        "realized": {"value": "0", "currency": "USD"},
        "cashValue": {"value": "0", "currency": "USD"},
        "expired": expired,
        "updateTime": "2026-10-20T00:00:00Z",
        "marketMetadata": {"slug": "", "title": title, "outcome": outcome, "eventSlug": ""}
    })
}

async fn mount_bbo(server: &MockServer, slug: &str, market_data: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/markets/{slug}/bbo")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"marketData": market_data})))
        .mount(server)
        .await;
}

#[tokio::test]
async fn sync_values_cash_and_open_positions() -> Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/account/balances"))
        .and(ValidSignature)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "balances": [{"currentBalance": 105.7, "currency": "USD", "buyingPower": 4.7}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/portfolio/positions"))
        .and(ValidSignature)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "positions": {
                "long-market": position("10", "4.5", false, "Will A happen?", "Yes"),
                "short-market": position("-2", "0.6", false, "Will B happen?", ""),
                "settled-market": position("5", "2", true, "Old market", "Yes"),
                "closed-market": position("0", "0", false, "Closed out", "Yes"),
            },
            "eof": true
        })))
        .mount(&server)
        .await;

    mount_bbo(
        &server,
        "long-market",
        json!({"currentPx": {"value": "0.6325", "currency": "USD"}}),
    )
    .await;
    mount_bbo(
        &server,
        "short-market",
        json!({"currentPx": null, "lastTradePx": {"value": "0.70", "currency": "USD"}}),
    )
    .await;

    let storage = MemoryStorage::new();
    let mut connection = test_connection();
    let result = test_synchronizer(&server)
        .sync(&mut connection, &storage)
        .await?;

    assert_eq!(result.accounts.len(), 1);
    assert_eq!(result.accounts[0].name, "Polymarket US");
    assert_eq!(
        connection.state.account_ids,
        vec![result.accounts[0].id.clone()]
    );

    let balances: Vec<_> = result.balances[0]
        .1
        .balances()
        .iter()
        .map(|b| {
            (
                serde_json::to_value(&b.asset_balance.asset).unwrap(),
                b.asset_balance.amount.clone(),
                b.asset_balance.cost_basis.clone(),
                b.price.as_ref().map(|p| p.price.clone()),
            )
        })
        .collect();
    let contract = |slug: &str, side: &str, name: &str| json!({"type": "polymarket_us", "slug": slug, "side": side, "name": name});
    let some = |v: &str| Some(v.to_string());
    assert_eq!(
        balances,
        vec![
            (
                // 105.7 fiat less the $1 margin locked per short contract.
                json!({"type": "currency", "iso_code": "USD"}),
                "103.7".to_string(),
                None,
                None
            ),
            (
                contract("long-market", "yes", "Will A happen? (Yes)"),
                "10".to_string(),
                some("4.5"),
                some("0.6325"),
            ),
            (
                contract("short-market", "no", "Will B happen? (No)"),
                "2".to_string(),
                some("0.6"),
                some("0.3"),
            ),
        ]
    );

    Ok(())
}

#[tokio::test]
async fn get_positions_follows_cursor_until_eof() -> Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/v1/portfolio/positions"))
        .and(query_param_is_missing("cursor"))
        .and(ValidSignature)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "positions": {"first": position("1", "0.5", false, "First", "Yes")},
            "nextCursor": "page-2",
            "eof": false
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/v1/portfolio/positions"))
        .and(query_param("cursor", "page-2"))
        .and(ValidSignature)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "positions": {"second": position("2", "1", false, "Second", "Yes")},
            "nextCursor": "",
            "eof": true
        })))
        .expect(1)
        .mount(&server)
        .await;

    let positions = test_synchronizer(&server).get_positions().await?;
    let slugs: Vec<_> = positions.iter().map(|(slug, _)| slug.as_str()).collect();
    assert_eq!(slugs, vec!["first", "second"]);

    Ok(())
}

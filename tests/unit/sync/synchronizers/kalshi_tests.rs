use super::*;
use crate::models::ConnectionConfig;
use crate::storage::MemoryStorage;
use ed25519_dalek::pkcs8::EncodePrivateKey as _;
use ed25519_dalek::Verifier as _;
use rsa::pkcs1::EncodeRsaPrivateKey as _;
use rsa::pkcs8::LineEnding;
use serde_json::json;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Match, Mock, MockServer, Request, ResponseTemplate};

const SEED: [u8; 32] = [7; 32];

fn ed25519_key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&SEED)
}

fn test_synchronizer(server: &MockServer) -> KalshiSynchronizer {
    let pem = ed25519_key().to_pkcs8_pem(LineEnding::LF).unwrap();
    KalshiSynchronizer::new(
        "test-key-id".to_string(),
        SecretString::new(pem.to_string().into()),
    )
    .unwrap()
    .with_base_url(server.uri())
}

fn test_connection() -> Connection {
    Connection::new(ConnectionConfig {
        name: "Kalshi".to_string(),
        synchronizer: "kalshi".to_string(),
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
            header("KALSHI-ACCESS-KEY"),
            header("KALSHI-ACCESS-TIMESTAMP"),
            header("KALSHI-ACCESS-SIGNATURE"),
        ) else {
            return false;
        };
        let Some(signature) = STANDARD
            .decode(signature)
            .ok()
            .and_then(|bytes| ed25519_dalek::Signature::from_slice(&bytes).ok())
        else {
            return false;
        };
        let message = format!("{timestamp}{}{}", request.method, request.url.path());
        key_id == "test-key-id"
            && ed25519_key()
                .verifying_key()
                .verify(message.as_bytes(), &signature)
                .is_ok()
    }
}

fn position(ticker: &str, position: &str, exposure: &str) -> serde_json::Value {
    json!({
        "ticker": ticker,
        "exchange_index": 0,
        "total_traded_dollars": exposure,
        "position_fp": position,
        "market_exposure_dollars": exposure,
        "realized_pnl_dollars": "0.0000",
        "fees_paid_dollars": "0.0100",
        "last_updated_ts": "2026-10-20T00:00:00Z"
    })
}

#[tokio::test]
async fn sync_values_cash_and_open_positions() -> Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/trade-api/v2/portfolio/balance"))
        .and(ValidSignature)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "balance": 10570,
            "balance_dollars": "105.7000",
            "portfolio_value": 900,
            "updated_ts": 1792540800
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/trade-api/v2/portfolio/positions"))
        .and(query_param_is_missing("cursor"))
        .and(ValidSignature)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "market_positions": [
                position("YES-MKT", "10.00", "4.5000"),
                position("CLOSED-MKT", "0.00", "0.0000"),
            ],
            "event_positions": [],
            "cursor": "page-2"
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/trade-api/v2/portfolio/positions"))
        .and(query_param("cursor", "page-2"))
        .and(ValidSignature)
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "market_positions": [
                position("NO-MKT", "-4.00", "1.2000"),
                position("DETERMINED-MKT", "3.00", "2.1000"),
            ],
            "event_positions": [],
            "cursor": ""
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/trade-api/v2/markets"))
        .and(query_param("tickers", "YES-MKT,NO-MKT,DETERMINED-MKT"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "markets": [
                {
                    "ticker": "YES-MKT",
                    "title": "Will A happen?",
                    "yes_sub_title": "Will A happen?",
                    "yes_bid_dollars": "0.6200",
                    "yes_ask_dollars": "0.6400",
                    "last_price_dollars": "0.6000",
                    "notional_value_dollars": "1.0000",
                    "settlement_value_dollars": null
                },
                {
                    "ticker": "NO-MKT",
                    "title": "Fed rate in December",
                    "yes_sub_title": "Above 4%",
                    "no_sub_title": "Not above 4%",
                    "no_bid_dollars": "0.0000",
                    "no_ask_dollars": "0.2800",
                    "last_price_dollars": "0.7500",
                    "notional_value_dollars": "1.0000"
                },
                {
                    "ticker": "DETERMINED-MKT",
                    "title": "Will C happen?",
                    "yes_bid_dollars": "0.0000",
                    "yes_ask_dollars": "0.0000",
                    "last_price_dollars": "0.9900",
                    "notional_value_dollars": "1.0000",
                    "settlement_value_dollars": "1.0000"
                }
            ],
            "cursor": ""
        })))
        .expect(1)
        .mount(&server)
        .await;

    let storage = MemoryStorage::new();
    let mut connection = test_connection();
    let result = test_synchronizer(&server)
        .sync(&mut connection, &storage)
        .await?;

    assert_eq!(result.accounts.len(), 1);
    assert_eq!(result.accounts[0].name, "Kalshi");
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
    let contract = |ticker: &str, side: &str, name: &str| json!({"type": "kalshi", "ticker": ticker, "side": side, "name": name});
    let some = |v: &str| Some(v.to_string());
    assert_eq!(
        balances,
        vec![
            (
                json!({"type": "currency", "iso_code": "USD"}),
                "105.7".to_string(),
                None,
                None
            ),
            (
                contract("YES-MKT", "yes", "Will A happen? (Yes)"),
                "10".to_string(),
                some("4.5"),
                some("0.63"),
            ),
            (
                contract("NO-MKT", "no", "Fed rate in December (No: Not above 4%)"),
                "4".to_string(),
                some("1.2"),
                some("0.25"),
            ),
            (
                contract("DETERMINED-MKT", "yes", "Will C happen? (Yes)"),
                "3".to_string(),
                some("2.1"),
                some("1"),
            ),
        ]
    );

    Ok(())
}

#[test]
fn rsa_keys_sign_with_pss_sha256() -> Result<()> {
    use rsa::signature::Verifier as _;

    let private_key = RsaPrivateKey::new(&mut rand::thread_rng(), 1024)?;
    let pem = private_key.to_pkcs1_pem(LineEnding::LF)?;
    let signer = KalshiSigner::from_pem(&pem)?;
    assert!(matches!(signer, KalshiSigner::Rsa(_)));

    let message = b"1792540800000GET/trade-api/v2/portfolio/balance";
    let signature = rsa::pss::Signature::try_from(signer.sign(message).as_slice())?;
    rsa::pss::VerifyingKey::<Sha256>::new(private_key.to_public_key())
        .verify(message, &signature)?;

    Ok(())
}

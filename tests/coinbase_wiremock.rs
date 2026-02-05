use anyhow::Result;
use chrono::Utc;
use keepbook::models::{Asset, Connection, ConnectionConfig};
use keepbook::storage::JsonFileStorage;
use keepbook::sync::synchronizers::CoinbaseSynchronizer;
use secrecy::SecretString;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use p256::SecretKey;
use rand::rngs::OsRng;

fn test_private_key_pem() -> SecretString {
    let secret = SecretKey::random(&mut OsRng);
    let pem = secret
        .to_sec1_pem(Default::default())
        .expect("failed to render pem");
    SecretString::new(pem.to_string().into())
}

#[tokio::test]
async fn coinbase_sync_filters_zero_balances_and_cash_positions() -> Result<()> {
    let server = MockServer::start().await;

    let portfolios_body = r#"{
        "portfolios": [
            { "uuid": "p1", "name": "Main" }
        ]
    }"#;
    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/portfolios"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(portfolios_body, "application/json"))
        .mount(&server)
        .await;

    let breakdown_body = r#"{
        "breakdown": {
            "spot_positions": [
                {
                    "asset": "BTC",
                    "account_uuid": "acct-zero",
                    "total_balance_crypto": 0.0,
                    "is_cash": false
                },
                {
                    "asset": "ETH",
                    "account_uuid": "acct-eth",
                    "total_balance_crypto": 0.5,
                    "is_cash": false
                },
                {
                    "asset": "USD",
                    "account_uuid": "acct-usd",
                    "total_balance_crypto": 25.0,
                    "is_cash": true
                }
            ]
        }
    }"#;
    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/portfolios/p1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(breakdown_body, "application/json"))
        .mount(&server)
        .await;

    let ledger_empty = r#"{"ledger": []}"#;
    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/accounts/acct-zero/ledger"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(ledger_empty, "application/json"))
        .mount(&server)
        .await;

    let ledger_body = r#"{
        "ledger": [
            {
                "entry_id": "entry-1",
                "entry_type": "trade",
                "amount": { "value": "0.5", "currency": "ETH" },
                "created_at": "2024-01-02T00:00:00Z",
                "description": "buy"
            }
        ]
    }"#;
    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/accounts/acct-eth/ledger"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(ledger_body, "application/json"))
        .mount(&server)
        .await;

    let ledger_usd = r#"{"ledger": []}"#;
    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/accounts/acct-usd/ledger"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(ledger_usd, "application/json"))
        .mount(&server)
        .await;

    let temp = TempDir::new()?;
    let storage = JsonFileStorage::new(temp.path());

    let mut connection = Connection::new(ConnectionConfig {
        name: "Coinbase".to_string(),
        synchronizer: "coinbase".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    let synchronizer = CoinbaseSynchronizer::new("key".to_string(), test_private_key_pem())
        .with_base_url(server.uri());

    let result = synchronizer
        .sync_with_storage(&mut connection, &storage)
        .await?;

    assert_eq!(result.accounts.len(), 1);
    let account = &result.accounts[0];
    assert_eq!(account.name, "ETH Wallet");
    assert_eq!(
        account.synchronizer_data.get("currency").and_then(|v| v.as_str()),
        Some("ETH")
    );

    let (_, balances) = &result.balances[0];
    assert!(balances.iter().any(|b| {
        matches!(b.asset_balance.asset, Asset::Crypto { ref symbol, .. } if symbol == "ETH")
    }));

    let (_, txns) = &result.transactions[0];
    assert_eq!(txns.len(), 1);
    assert_eq!(txns[0].description, "buy");
    assert!(txns[0].timestamp <= Utc::now());

    Ok(())
}

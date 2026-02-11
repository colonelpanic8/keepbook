use anyhow::Result;
use chrono::Utc;
use keepbook::models::{Asset, Connection, ConnectionConfig, Id};
use keepbook::storage::{JsonFileStorage, Storage};
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

    let fills_body = r#"{
        "fills": [
            {
                "entry_id": "entry-1",
                "product_id": "ETH-USD",
                "size": "0.5",
                "trade_time": "2024-01-02T00:00:00Z",
                "side": "BUY"
            }
        ],
        "has_next": false
    }"#;
    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/orders/historical/fills"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(fills_body, "application/json"))
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
        account
            .synchronizer_data
            .get("currency")
            .and_then(|v| v.as_str()),
        Some("ETH")
    );

    let (_, balances) = &result.balances[0];
    assert!(balances.iter().any(|b| {
        matches!(b.asset_balance.asset, Asset::Crypto { ref symbol, .. } if symbol == "ETH")
    }));

    let (_, txns) = &result.transactions[0];
    assert_eq!(txns.len(), 1);
    assert_eq!(txns[0].description, "BUY ETH-USD");
    assert!(txns[0].timestamp <= Utc::now());

    Ok(())
}

#[tokio::test]
async fn coinbase_keeps_zero_balance_with_transactions() -> Result<()> {
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
                }
            ]
        }
    }"#;
    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/portfolios/p1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(breakdown_body, "application/json"))
        .mount(&server)
        .await;

    let fills_body = r#"{
        "fills": [
            {
                "entry_id": "entry-1",
                "product_id": "BTC-USD",
                "size": "0.01",
                "trade_time": "2024-01-02T00:00:00Z",
                "side": "BUY"
            }
        ],
        "has_next": false
    }"#;
    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/orders/historical/fills"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(fills_body, "application/json"))
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
    assert_eq!(result.transactions.len(), 1);
    assert_eq!(result.transactions[0].1.len(), 1);
    assert_eq!(result.transactions[0].1[0].description, "BUY BTC-USD");

    Ok(())
}

#[tokio::test]
async fn coinbase_keeps_existing_zero_balance_without_transactions() -> Result<()> {
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
                }
            ]
        }
    }"#;
    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/portfolios/p1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(breakdown_body, "application/json"))
        .mount(&server)
        .await;

    let fills_body = r#"{"fills": [], "has_next": false}"#;
    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/orders/historical/fills"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(fills_body, "application/json"))
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

    let mut existing = keepbook::models::Account::new("Existing", connection.id().clone());
    existing.id = keepbook::models::Id::from_string("acct-zero");
    storage.save_account(&existing).await?;

    let synchronizer = CoinbaseSynchronizer::new("key".to_string(), test_private_key_pem())
        .with_base_url(server.uri());

    let result = synchronizer
        .sync_with_storage(&mut connection, &storage)
        .await?;

    assert_eq!(result.accounts.len(), 1);
    assert_eq!(result.accounts[0].id.to_string(), "acct-zero");

    Ok(())
}

#[tokio::test]
async fn coinbase_account_ids_are_stable() -> Result<()> {
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
                    "asset": "ETH",
                    "account_uuid": "acct-eth",
                    "total_balance_crypto": 0.5,
                    "is_cash": false
                }
            ]
        }
    }"#;
    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/portfolios/p1"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(breakdown_body, "application/json"))
        .mount(&server)
        .await;

    let fills_body = r#"{"fills": [], "has_next": false}"#;
    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/orders/historical/fills"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(fills_body, "application/json"))
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
    assert_eq!(result.accounts[0].id, Id::from_string("acct-eth"));

    Ok(())
}

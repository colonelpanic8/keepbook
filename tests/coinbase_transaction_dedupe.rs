use anyhow::{Context, Result};
use keepbook::models::{Connection, ConnectionConfig};
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::synchronizers::CoinbaseSynchronizer;
use keepbook::sync::Synchronizer;
use p256::elliptic_curve::rand_core::OsRng;
use p256::pkcs8::LineEnding;
use p256::SecretKey;
use secrecy::SecretString;
use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn coinbase_sync_dedupes_transactions_by_id() -> Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/portfolios"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "portfolios": [{"uuid": "p1", "name": "Default"}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/portfolios/p1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "breakdown": {
                "spot_positions": [{
                    "asset": "BTC",
                    "account_uuid": "11111111-1111-1111-1111-111111111111",
                    "total_balance_crypto": 0.5,
                    "is_cash": false
                }]
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/orders/historical/fills"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "fills": [{
                "entry_id": "entry-1",
                "product_id": "BTC-USD",
                "size": "0.01",
                "trade_time": "2024-01-02T03:04:05Z",
                "side": "SELL"
            }],
            "has_next": false
        })))
        .mount(&server)
        .await;

    let secret_key = SecretKey::random(&mut OsRng);
    let pem = secret_key
        .to_sec1_pem(LineEnding::LF)
        .context("Failed to encode test EC private key")?;

    let synchronizer = CoinbaseSynchronizer::new(
        "test-key".to_string(),
        SecretString::new(pem.to_string().into()),
    )
    .with_base_url(server.uri());

    let dir = TempDir::new()?;
    let storage = JsonFileStorage::new(dir.path());

    let mut connection = Connection::new(ConnectionConfig {
        name: "Coinbase".to_string(),
        synchronizer: "coinbase".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;

    for _ in 0..2 {
        let result = synchronizer.sync(&mut connection, &storage).await?;
        result.save(&storage).await?;
    }

    let loaded = storage
        .get_connection(connection.id())
        .await?
        .context("connection should exist")?;
    assert_eq!(loaded.state.account_ids.len(), 1);

    let account_id = loaded.state.account_ids[0].clone();
    let transactions = storage.get_transactions(&account_id).await?;
    assert_eq!(
        transactions.len(),
        1,
        "same entry should not be stored twice"
    );

    Ok(())
}

use anyhow::{Context, Result};
use keepbook::models::{Connection, ConnectionConfig, Id};
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::Synchronizer;
use keepbook::sync::synchronizers::CoinbaseSynchronizer;
use p256::elliptic_curve::rand_core::OsRng;
use p256::pkcs8::LineEnding;
use p256::SecretKey;
use secrecy::SecretString;
use serde_json::json;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn coinbase_sync_handles_unsafe_account_uuid() -> Result<()> {
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
                    "account_uuid": "bad/id",
                    "total_balance_crypto": 0.5,
                    "is_cash": false
                }]
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/accounts/bad/id/ledger"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ledger": []
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

    let result = synchronizer.sync(&mut connection, &storage).await?;

    // Saving should not fail due to path-unsafe account ids.
    result.save(&storage).await?;

    let loaded = storage
        .get_connection(connection.id())
        .await?
        .context("connection should exist")?;

    assert_eq!(loaded.state.account_ids.len(), 1);
    let account_id = &loaded.state.account_ids[0];
    assert!(
        Id::is_path_safe(account_id.as_str()),
        "account id should be path-safe"
    );

    Ok(())
}

use super::*;
use crate::models::ConnectionConfig;
use crate::storage::MemoryStorage;
use p256::elliptic_curve::rand_core::OsRng;
use p256::pkcs8::LineEnding;
use serde_json::json;
use wiremock::matchers::{method, path, query_param, query_param_is_missing};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn request_invalid_http_method_returns_error_not_panic() {
    // The request path validates HTTP method before trying to parse the private key.
    let synchronizer = CoinbaseSynchronizer::new(
        "test-key".to_string(),
        SecretString::new("not a real pem".to_string().into()),
    );

    let err = synchronizer
        // Spaces are not allowed in HTTP method tokens, so parsing must fail.
        .request::<serde_json::Value>("NOT A METHOD", "/api/v3/brokerage/portfolios")
        .await
        .unwrap_err();

    assert!(err.to_string().contains("Invalid HTTP method"));
}

#[tokio::test]
async fn sync_works_against_wiremock() -> Result<()> {
    // This is a "real" integration-style unit test: it exercises the actual HTTP code paths,
    // but with a local Wiremock server instead of hitting the network.
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
            "fills": [],
            "has_next": false
        })))
        .mount(&server)
        .await;

    // Generate a throwaway P-256 key and encode it in the SEC1 PEM format the sync code expects.
    let secret_key = SecretKey::random(&mut OsRng);
    let pem = secret_key
        .to_sec1_pem(LineEnding::LF)
        .context("Failed to encode test EC private key")?;

    let synchronizer = CoinbaseSynchronizer::new(
        "test-key".to_string(),
        SecretString::new(pem.to_string().into()),
    )
    .with_base_url(server.uri());

    let storage = MemoryStorage::new();
    let mut connection = Connection::new(ConnectionConfig {
        name: "Coinbase".to_string(),
        synchronizer: "coinbase".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    let result = synchronizer.sync(&mut connection, &storage).await?;

    assert_eq!(result.accounts.len(), 1);
    assert_eq!(result.accounts[0].name, "BTC Wallet");
    assert_eq!(result.balances.len(), 1);
    assert_eq!(result.balances[0].1.len(), 1);
    assert!(matches!(
        result.balances[0].1[0].asset_balance.asset,
        Asset::Crypto { .. }
    ));
    assert_eq!(result.balances[0].1[0].asset_balance.amount, "0.5");
    assert_eq!(result.transactions.len(), 1);
    assert!(result.transactions[0].1.is_empty());

    Ok(())
}

#[tokio::test]
async fn sync_preserves_coinbase_cash_positions() -> Result<()> {
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
                "spot_positions": [
                    {
                        "asset": "USD",
                        "account_uuid": "22222222-2222-2222-2222-222222222222",
                        "total_balance_fiat": 123.45,
                        "total_balance_crypto": 123.45,
                        "is_cash": true
                    },
                    {
                        "asset": "USDC",
                        "account_uuid": "33333333-3333-3333-3333-333333333333",
                        "total_balance_fiat": 0.01,
                        "total_balance_crypto": 0.013745,
                        "is_cash": true
                    }
                ]
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/orders/historical/fills"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "fills": [],
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

    let storage = MemoryStorage::new();
    let mut connection = Connection::new(ConnectionConfig {
        name: "Coinbase".to_string(),
        synchronizer: "coinbase".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    let result = synchronizer.sync(&mut connection, &storage).await?;

    assert_eq!(result.accounts.len(), 2);
    assert_eq!(result.accounts[0].name, "USD Cash");
    assert_eq!(result.accounts[0].tags[1], "ACCOUNT_TYPE_FIAT");
    assert_eq!(result.accounts[1].name, "USDC Wallet");
    assert_eq!(result.accounts[1].tags[1], "ACCOUNT_TYPE_CRYPTO");
    assert_eq!(result.balances.len(), 2);
    assert!(matches!(
        result.balances[0].1[0].asset_balance.asset,
        Asset::Currency { .. }
    ));
    assert_eq!(result.balances[0].1[0].asset_balance.amount, "123.45");
    assert!(matches!(
        result.balances[1].1[0].asset_balance.asset,
        Asset::Crypto { .. }
    ));
    assert_eq!(result.balances[1].1[0].asset_balance.amount, "0.013745");

    Ok(())
}

#[tokio::test]
async fn sync_maps_coinbase_fills_to_wallet_transactions() -> Result<()> {
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
                    "total_balance_crypto": 1.25,
                    "is_cash": false
                }]
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/orders/historical/fills"))
        .and(query_param_is_missing("cursor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "fills": [{
                "entry_id": "entry-1",
                "trade_id": "trade-1",
                "order_id": "order-1",
                "product_id": "BTC-USD",
                "size": "0.10",
                "trade_time": "2026-02-10T12:34:56Z",
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

    let storage = MemoryStorage::new();
    let mut connection = Connection::new(ConnectionConfig {
        name: "Coinbase".to_string(),
        synchronizer: "coinbase".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    let result = synchronizer.sync(&mut connection, &storage).await?;
    let txs = &result.transactions[0].1;
    assert_eq!(txs.len(), 1);
    assert_eq!(txs[0].amount, "-0.10");
    assert_eq!(txs[0].description, "SELL BTC-USD");
    assert_eq!(
        txs[0]
            .synchronizer_data
            .get("coinbase_entry_id")
            .and_then(|v| v.as_str()),
        Some("entry-1")
    );

    Ok(())
}

#[tokio::test]
async fn get_fills_paginates_on_cursor() -> Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/orders/historical/fills"))
        .and(query_param_is_missing("cursor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "fills": [{
                "entry_id": "entry-1",
                "product_id": "BTC-USD",
                "size": "0.01",
                "trade_time": "2026-02-10T00:00:00Z",
                "side": "BUY"
            }],
            "has_next": true,
            "cursor": "abc123"
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/v3/brokerage/orders/historical/fills"))
        .and(query_param("cursor", "abc123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "fills": [{
                "entry_id": "entry-2",
                "product_id": "BTC-USD",
                "size": "0.02",
                "trade_time": "2026-02-10T01:00:00Z",
                "side": "BUY"
            }],
            "has_next": false
        })))
        .expect(1)
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

    let fills = synchronizer.get_fills().await?;
    assert_eq!(fills.len(), 2);
    assert_eq!(fills[0].entry_id.as_deref(), Some("entry-1"));
    assert_eq!(fills[1].entry_id.as_deref(), Some("entry-2"));

    Ok(())
}

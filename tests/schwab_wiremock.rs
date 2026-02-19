use std::collections::HashMap;

use anyhow::Result;
use chrono::Duration;
use keepbook::credentials::{SessionCache, SessionData};
use keepbook::market_data::PriceKind;
use keepbook::models::{Account, Asset, Connection, ConnectionConfig, Id};
use keepbook::storage::{JsonFileStorage, Storage};
use keepbook::sync::synchronizers::SchwabSynchronizer;
use tempfile::TempDir;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SCHWAB_TXN_HISTORY_PATH: &str =
    "/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/brokerage/transactions";

async fn mount_empty_transactions_mock(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path(SCHWAB_TXN_HISTORY_PATH))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"brokerageTransactions":[],"bookmark":null}"#,
            "application/json",
        ))
        .mount(server)
        .await;
}

#[tokio::test]
async fn schwab_sync_uses_cached_session_and_base_url_override() -> Result<()> {
    let server = MockServer::start().await;

    let accounts_body = r#"{
        "Accounts": [
            {
                "AccountId": "ABC123",
                "AccountNumberDisplay": "1234",
                "AccountNumberDisplayFull": "000011112222",
                "DefaultName": "Schwab Brokerage",
                "NickName": "",
                "AccountType": "Brokerage",
                "IsBrokerage": true,
                "IsBank": false,
                "Balances": {
                    "Balance": 1000.0,
                    "DayChange": 0.0,
                    "DayChangePct": 0.0,
                    "Cash": 250.0,
                    "MarketValue": 750.0
                }
            }
        ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/Account"))
        .and(query_param("includeCustomGroups", "true"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(accounts_body, "application/json"))
        .mount(&server)
        .await;

    let positions_body = r#"{
        "SecurityGroupings": [
            {
                "GroupName": "Equities",
                "Positions": [
                    {
                        "DefaultSymbol": "AAPL",
                        "Description": "Apple Inc",
                        "Quantity": 5.0,
                        "Price": 150.0,
                        "MarketValue": 750.0,
                        "Cost": 600.0,
                        "ProfitLoss": 150.0,
                        "ProfitLossPercent": 25.0,
                        "DayChange": 0.0,
                        "PercentDayChange": 0.0
                    },
                    {
                        "DefaultSymbol": "CASH",
                        "Description": "Cash",
                        "Quantity": 250.0,
                        "Price": 1.0,
                        "MarketValue": 250.0,
                        "Cost": 250.0,
                        "ProfitLoss": 0.0,
                        "ProfitLossPercent": 0.0,
                        "DayChange": 0.0,
                        "PercentDayChange": 0.0
                    }
                ]
            }
        ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/AggregatedPositions"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(positions_body, "application/json"))
        .mount(&server)
        .await;
    mount_empty_transactions_mock(&server).await;

    let data_dir = TempDir::new()?;
    let storage = JsonFileStorage::new(data_dir.path());

    let mut connection = Connection::new(ConnectionConfig {
        name: "Schwab".to_string(),
        synchronizer: "schwab".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    let cache_dir = TempDir::new()?;
    let session_cache = SessionCache::with_path(cache_dir.path())?;

    let mut session = SessionData::new().with_token("test-token");
    session.data.insert("api_base".to_string(), server.uri());
    session.cookies = HashMap::new();
    session_cache.set(&connection.id().to_string(), &session)?;

    let synchronizer = SchwabSynchronizer::with_session_cache(&connection, session_cache);
    let result = synchronizer
        .sync_with_storage(&mut connection, &storage)
        .await?;

    assert_eq!(result.accounts.len(), 1);
    let account = &result.accounts[0];
    assert_eq!(account.name, "Schwab Brokerage");

    let (_, balances) = &result.balances[0];
    let aapl_balance = balances.iter().find(|b| {
        matches!(
            b.asset_balance.asset,
            Asset::Equity { ref ticker, .. } if ticker == "AAPL"
        )
    });
    let has_cash = balances.iter().any(|b| {
        matches!(
            b.asset_balance.asset,
            Asset::Currency { ref iso_code } if iso_code == "USD"
        )
    });
    let has_cash_position = balances.iter().any(|b| {
        matches!(
            b.asset_balance.asset,
            Asset::Equity { ref ticker, .. } if ticker == "CASH"
        )
    });

    let aapl_balance = aapl_balance.expect("expected equity position balance");
    let price = aapl_balance
        .price
        .as_ref()
        .expect("expected price for equity position");
    assert_eq!(price.quote_currency, "USD");
    assert_eq!(price.source, "schwab");
    assert_eq!(price.kind, PriceKind::Close);
    let price_value: f64 = price.price.parse().expect("price should parse");
    assert_eq!(price_value, 150.0);
    assert!(has_cash, "expected cash balance from account balances");
    assert!(!has_cash_position, "cash position should be skipped");

    Ok(())
}

#[tokio::test]
async fn schwab_preserves_created_at_for_existing_account() -> Result<()> {
    let server = MockServer::start().await;

    let accounts_body = r#"{
        "Accounts": [
            {
                "AccountId": "ABC123",
                "AccountNumberDisplay": "1234",
                "AccountNumberDisplayFull": "000011112222",
                "DefaultName": "Schwab Brokerage",
                "NickName": "",
                "AccountType": "Brokerage",
                "IsBrokerage": true,
                "IsBank": false,
                "Balances": {
                    "Balance": 1000.0,
                    "DayChange": 0.0,
                    "DayChangePct": 0.0,
                    "Cash": 250.0,
                    "MarketValue": 750.0
                }
            }
        ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/Account"))
        .and(query_param("includeCustomGroups", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(accounts_body, "application/json"))
        .mount(&server)
        .await;

    let positions_body = r#"{
        "SecurityGroupings": [
            {
                "GroupName": "Equities",
                "Positions": []
            }
        ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/AggregatedPositions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(positions_body, "application/json"))
        .mount(&server)
        .await;
    mount_empty_transactions_mock(&server).await;

    let data_dir = TempDir::new()?;
    let storage = JsonFileStorage::new(data_dir.path());

    let mut connection = Connection::new(ConnectionConfig {
        name: "Schwab".to_string(),
        synchronizer: "schwab".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    let mut existing = Account::new("Existing", connection.id().clone());
    existing.id = Id::from_external("ABC123");
    let original_created_at = existing.created_at - Duration::days(10);
    existing.created_at = original_created_at;
    storage.save_account(&existing).await?;

    let cache_dir = TempDir::new()?;
    let session_cache = SessionCache::with_path(cache_dir.path())?;
    let mut session = SessionData::new().with_token("test-token");
    session.data.insert("api_base".to_string(), server.uri());
    session_cache.set(&connection.id().to_string(), &session)?;

    let synchronizer = SchwabSynchronizer::with_session_cache(&connection, session_cache);
    let result = synchronizer
        .sync_with_storage(&mut connection, &storage)
        .await?;

    assert_eq!(result.accounts.len(), 1);
    assert_eq!(result.accounts[0].id, existing.id);
    assert_eq!(result.accounts[0].created_at, original_created_at);

    Ok(())
}

#[tokio::test]
async fn schwab_account_ids_are_deterministic() -> Result<()> {
    let server = MockServer::start().await;

    let accounts_body = r#"{
        "Accounts": [
            {
                "AccountId": "ABC123",
                "AccountNumberDisplay": "1234",
                "AccountNumberDisplayFull": "000011112222",
                "DefaultName": "Schwab Brokerage",
                "NickName": "",
                "AccountType": "Brokerage",
                "IsBrokerage": true,
                "IsBank": false,
                "Balances": {
                    "Balance": 1000.0,
                    "DayChange": 0.0,
                    "DayChangePct": 0.0,
                    "Cash": 250.0,
                    "MarketValue": 750.0
                }
            }
        ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/Account"))
        .and(query_param("includeCustomGroups", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(accounts_body, "application/json"))
        .mount(&server)
        .await;

    let positions_body = r#"{
        "SecurityGroupings": [
            {
                "GroupName": "Equities",
                "Positions": []
            }
        ]
    }"#;

    Mock::given(method("GET"))
        .and(path("/AggregatedPositions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(positions_body, "application/json"))
        .mount(&server)
        .await;
    mount_empty_transactions_mock(&server).await;

    let data_dir = TempDir::new()?;
    let storage = JsonFileStorage::new(data_dir.path());

    let mut connection = Connection::new(ConnectionConfig {
        name: "Schwab".to_string(),
        synchronizer: "schwab".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    let cache_dir = TempDir::new()?;
    let session_cache = SessionCache::with_path(cache_dir.path())?;
    let mut session = SessionData::new().with_token("test-token");
    session.data.insert("api_base".to_string(), server.uri());
    session_cache.set(&connection.id().to_string(), &session)?;

    let synchronizer = SchwabSynchronizer::with_session_cache(&connection, session_cache);
    let result = synchronizer
        .sync_with_storage(&mut connection, &storage)
        .await?;

    assert_eq!(result.accounts.len(), 1);
    assert_eq!(result.accounts[0].id, Id::from_external("ABC123"));

    Ok(())
}

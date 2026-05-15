use super::*;
use crate::models::{ConnectionConfig, TransactionStatus};
use crate::storage::MemoryStorage;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn sync_updates_cursor_and_handles_removed_transactions() -> Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/accounts/balance/get"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accounts": [{
                "account_id": "acc_1",
                "name": "Checking",
                "type": "depository",
                "subtype": "checking",
                "mask": "0000",
                "official_name": "Primary Checking",
                "balances": {
                    "current": 1000.25,
                    "iso_currency_code": "USD"
                }
            }]
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/transactions/sync"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "added": [{
                "transaction_id": "tx_added",
                "account_id": "acc_1",
                "amount": 12.34,
                "date": "2026-02-10",
                "name": "Coffee",
                "pending": false,
                "iso_currency_code": "USD"
            }],
            "modified": [],
            "removed": [{
                "transaction_id": "tx_removed"
            }],
            "has_more": false,
            "next_cursor": "cursor-1"
        })))
        .mount(&server)
        .await;

    let storage = MemoryStorage::new();
    let mut connection = Connection::new(ConnectionConfig {
        name: "Plaid".to_string(),
        synchronizer: "plaid".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    connection.state.synchronizer_data = serde_json::json!({
        "access_token": "access-token-1",
        "transactions_cursor": "cursor-0",
    });

    let existing_account_id = Id::from_external("acc_1");
    let existing_account = Account {
        id: existing_account_id.clone(),
        name: "Existing Checking".to_string(),
        connection_id: connection.id().clone(),
        tags: vec!["plaid".to_string()],
        created_at: Utc::now() - chrono::Duration::days(30),
        active: true,
        synchronizer_data: serde_json::json!({
            "plaid_account_id": "acc_1"
        }),
    };
    storage.save_account(&existing_account).await?;

    let removed_tx = Transaction::new("-5", Asset::currency("USD"), "Old Tx")
        .with_id(Id::from_external("plaid:tx:tx_removed"))
        .with_status(TransactionStatus::Posted)
        .with_synchronizer_data(serde_json::json!({
            "plaid_transaction_id": "tx_removed",
        }));
    storage
        .append_transactions(&existing_account_id, &[removed_tx])
        .await?;

    let synchronizer = PlaidSynchronizer::new(
        SecretString::new("client-id".to_string().into()),
        SecretString::new("secret".to_string().into()),
        PlaidEnvironment::Sandbox,
    )
    .with_base_url(server.uri());

    let result = synchronizer.sync(&mut connection, &storage).await?;

    let synced_account = result
        .accounts
        .iter()
        .find(|a| a.id == existing_account_id)
        .context("Expected account in sync result")?;
    assert_eq!(synced_account.created_at, existing_account.created_at);

    let account_txs = result
        .transactions
        .iter()
        .find(|(id, _)| *id == existing_account_id)
        .map(|(_, txs)| txs)
        .context("Expected transactions for existing account")?;

    let added = account_txs
        .iter()
        .find(|tx| {
            tx.synchronizer_data
                .get("plaid_transaction_id")
                .and_then(|v| v.as_str())
                == Some("tx_added")
        })
        .context("Expected added transaction")?;
    assert_eq!(added.id, Id::from_external("plaid:tx:tx_added"));
    assert_eq!(added.amount, "-12.34");
    assert_eq!(added.status, TransactionStatus::Posted);

    let canceled = account_txs
        .iter()
        .find(|tx| {
            tx.synchronizer_data
                .get("plaid_transaction_id")
                .and_then(|v| v.as_str())
                == Some("tx_removed")
        })
        .context("Expected canceled transaction")?;
    assert_eq!(canceled.id, Id::from_external("plaid:tx:tx_removed"));
    assert_eq!(canceled.status, TransactionStatus::Canceled);
    assert_eq!(
        canceled.synchronizer_data.get("removed"),
        Some(&serde_json::json!(true))
    );

    assert_eq!(
        result.connection.state.synchronizer_data["transactions_cursor"],
        serde_json::json!("cursor-1")
    );
    assert_eq!(
        result.connection.state.synchronizer_data["environment"],
        serde_json::json!("sandbox")
    );

    Ok(())
}

#[tokio::test]
async fn sync_without_access_token_fails_fast() {
    let storage = MemoryStorage::new();
    let mut connection = Connection::new(ConnectionConfig {
        name: "Plaid".to_string(),
        synchronizer: "plaid".to_string(),
        credentials: None,
        balance_staleness: None,
    });

    let synchronizer = PlaidSynchronizer::new(
        SecretString::new("client-id".to_string().into()),
        SecretString::new("secret".to_string().into()),
        PlaidEnvironment::Sandbox,
    );

    let err = synchronizer
        .sync(&mut connection, &storage)
        .await
        .expect_err("sync should fail without access token");
    assert!(err.to_string().contains("No Plaid access token configured"));
}

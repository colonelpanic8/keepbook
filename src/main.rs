mod models;
mod storage;
mod sync;

use anyhow::Result;
use storage::{JsonFileStorage, Storage};
use sync::coinbase::CoinbaseSynchronizer;
use sync::Synchronizer;
use models::Connection;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Keepbook - Personal Finance Manager");
    println!("====================================\n");

    // Initialize storage
    let storage = JsonFileStorage::new("data");

    // Load or create Coinbase connection
    let connections = storage.list_connections().await?;
    let mut connection = connections
        .into_iter()
        .find(|c| c.synchronizer == "coinbase")
        .unwrap_or_else(|| Connection::new("Coinbase", "coinbase"));

    println!("Connection: {} ({})", connection.name, connection.id);

    // Initialize synchronizer from pass credentials
    println!("\nLoading Coinbase credentials from pass...");
    let synchronizer = CoinbaseSynchronizer::from_pass()?;

    // Perform sync
    println!("Syncing from Coinbase...\n");
    let result = synchronizer.sync(&mut connection).await?;

    // Save everything
    println!("Saving connection...");
    storage.save_connection(&result.connection).await?;

    println!("Saving {} accounts...", result.accounts.len());
    for account in &result.accounts {
        println!("  - {} ({})", account.name, account.id);
        storage.save_account(account).await?;
    }

    println!("\nSaving balances...");
    for (account_id, balances) in &result.balances {
        if !balances.is_empty() {
            println!("  - Account {}: {} balance(s)", account_id, balances.len());
            for balance in balances {
                println!("    {} {}", balance.amount, serde_json::to_string(&balance.asset)?);
            }
            storage.append_balances(account_id, balances).await?;
        }
    }

    println!("\nSaving transactions...");
    for (account_id, txns) in &result.transactions {
        if !txns.is_empty() {
            println!("  - Account {}: {} transaction(s)", account_id, txns.len());
            storage.append_transactions(account_id, txns).await?;
        }
    }

    println!("\nSync complete!");
    if let Some(last_sync) = &result.connection.last_sync {
        println!("Last sync: {} - {:?}", last_sync.at, last_sync.status);
    }

    Ok(())
}

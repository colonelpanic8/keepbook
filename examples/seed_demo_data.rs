//! Seed a throwaway keepbook data directory with demo accounts and
//! transactions for UI development and visual testing.
//!
//! Usage: cargo run --example seed_demo_data -- <target-dir>
//!
//! Writes <target-dir>/keepbook.toml and <target-dir>/data/... via the real
//! storage layer so file formats always match the current implementation.

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use keepbook::models::{
    Account, Asset, AssetBalance, BalanceSnapshot, Connection, ConnectionConfig, Id, Transaction,
    TransactionAnnotationPatch, TransactionStatus,
};
use keepbook::storage::{JsonFileStorage, Storage};

fn ts(date: &str) -> DateTime<Utc> {
    let date = date.parse::<NaiveDate>().expect("valid date literal");
    Utc.from_utc_datetime(&date.and_hms_opt(12, 0, 0).expect("valid time"))
}

fn tx(date: &str, amount: &str, description: &str, status: TransactionStatus) -> Transaction {
    let mut tx = Transaction::new(amount, Asset::currency("USD"), description);
    tx.timestamp = ts(date);
    tx.status = status;
    tx
}

fn tag_patch(transaction_id: &Id, date: &str, tags: &[&str]) -> TransactionAnnotationPatch {
    TransactionAnnotationPatch {
        transaction_id: transaction_id.clone(),
        timestamp: ts(date),
        description: None,
        note: None,
        tags: Some(Some(tags.iter().map(|tag| tag.to_string()).collect())),
        subtags: None,
        effective_date: None,
        ignore_spending: None,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let target = std::env::args()
        .nth(1)
        .context("usage: seed_demo_data <target-dir>")?;
    let target = std::path::PathBuf::from(target);
    let data_dir = target.join("data");
    std::fs::create_dir_all(&data_dir)?;
    std::fs::write(
        target.join("keepbook.toml"),
        "reporting_currency = \"USD\"\ndata_dir = \"data\"\n",
    )?;

    let storage = JsonFileStorage::new(&data_dir);

    let connection = Connection::new(ConnectionConfig {
        name: "Demo Bank".to_string(),
        synchronizer: "demo".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage.save_connection(&connection).await?;

    let checking = Account::new("Demo Checking", connection.id().clone());
    let card = Account::new("Demo Credit Card", connection.id().clone());
    storage.save_account(&checking).await?;
    storage.save_account(&card).await?;

    storage
        .append_balance_snapshot(
            &checking.id,
            &BalanceSnapshot::new(
                ts("2026-07-10"),
                vec![AssetBalance {
                    asset: Asset::currency("USD"),
                    amount: "8250.44".to_string(),
                    cost_basis: None,
                }],
            ),
        )
        .await?;
    storage
        .append_balance_snapshot(
            &card.id,
            &BalanceSnapshot::new(
                ts("2026-07-10"),
                vec![AssetBalance {
                    asset: Asset::currency("USD"),
                    amount: "-1436.20".to_string(),
                    cost_basis: None,
                }],
            ),
        )
        .await?;

    // Checking: rent, paychecks, utilities, a pending debit, and an internal
    // transfer carrying the legacy magic ignore tag.
    let mut checking_txs = Vec::new();
    for (date, amount, description) in [
        ("2026-05-01", "-2400.00", "Park Ave Apartments Rent"),
        ("2026-06-01", "-2400.00", "Park Ave Apartments Rent"),
        ("2026-07-01", "-2400.00", "Park Ave Apartments Rent"),
    ] {
        checking_txs.push(tx(date, amount, description, TransactionStatus::Posted));
    }
    for date in ["2026-05-15", "2026-06-15", "2026-06-30"] {
        checking_txs.push(tx(
            date,
            "5000.00",
            "RAILBIRD INC PAYROLL",
            TransactionStatus::Posted,
        ));
    }
    for (date, amount) in [("2026-05-18", "-118.63"), ("2026-06-18", "-124.09")] {
        checking_txs.push(tx(date, amount, "PG&E Utilities", TransactionStatus::Posted));
    }
    let pending = tx(
        "2026-07-11",
        "-54.20",
        "Pending Card Authorization",
        TransactionStatus::Pending,
    );
    checking_txs.push(pending);
    let transfer = tx(
        "2026-06-20",
        "-1000.00",
        "Transfer to Brokerage",
        TransactionStatus::Posted,
    );
    let transfer_id = transfer.id.clone();
    checking_txs.push(transfer);
    storage
        .append_transactions(&checking.id, &checking_txs)
        .await?;

    let rent_ids = checking_txs
        .iter()
        .filter(|tx| tx.description.contains("Rent"))
        .map(|tx| tx.id.clone())
        .collect::<Vec<_>>();
    let utility_ids = checking_txs
        .iter()
        .filter(|tx| tx.description.contains("PG&E"))
        .map(|tx| tx.id.clone())
        .collect::<Vec<_>>();
    let mut checking_patches = Vec::new();
    for id in &rent_ids {
        checking_patches.push(tag_patch(id, "2026-07-02", &["Housing"]));
    }
    for id in &utility_ids {
        checking_patches.push(tag_patch(id, "2026-07-02", &["Utilities"]));
    }
    // Legacy magic-tag ignore, to exercise back-compat in the UI toggle.
    checking_patches.push(tag_patch(&transfer_id, "2026-07-02", &["ignore_spending"]));
    storage
        .append_transaction_annotation_patches(&checking.id, &checking_patches)
        .await?;

    // Credit card: groceries, dining, subscriptions, a refund credit, an
    // explicit annotation-level ignore, and an effective-date override.
    let mut card_txs = Vec::new();
    for (date, amount, description) in [
        ("2026-05-04", "-92.31", "WHOLE FOODS MARKET"),
        ("2026-05-21", "-78.10", "TRADER JOE'S"),
        ("2026-06-08", "-104.55", "WHOLE FOODS MARKET"),
        ("2026-06-24", "-66.89", "TRADER JOE'S"),
        ("2026-07-06", "-88.42", "WHOLE FOODS MARKET"),
    ] {
        card_txs.push(tx(date, amount, description, TransactionStatus::Posted));
    }
    for (date, amount, description) in [
        ("2026-05-09", "-42.50", "SOUVLA SF"),
        ("2026-06-13", "-61.75", "NOPA RESTAURANT"),
        ("2026-07-03", "-38.20", "TARTINE BAKERY"),
    ] {
        card_txs.push(tx(date, amount, description, TransactionStatus::Posted));
    }
    card_txs.push(tx(
        "2026-06-01",
        "-15.99",
        "NETFLIX.COM",
        TransactionStatus::Posted,
    ));
    card_txs.push(tx(
        "2026-06-27",
        "45.00",
        "REFUND: REI RETURNS",
        TransactionStatus::Posted,
    ));
    let reimbursed = tx(
        "2026-06-16",
        "-220.00",
        "TEAM DINNER (REIMBURSED)",
        TransactionStatus::Posted,
    );
    let reimbursed_id = reimbursed.id.clone();
    card_txs.push(reimbursed);
    let late_charge = tx(
        "2026-07-02",
        "-330.00",
        "UNITED AIRLINES",
        TransactionStatus::Posted,
    );
    let late_charge_id = late_charge.id.clone();
    card_txs.push(late_charge);
    storage.append_transactions(&card.id, &card_txs).await?;

    let mut card_patches = Vec::new();
    for tx in &card_txs {
        if tx.description.contains("WHOLE FOODS") || tx.description.contains("TRADER JOE") {
            card_patches.push(tag_patch(&tx.id, "2026-07-02", &["Groceries"]));
        }
        if tx.description.contains("SOUVLA")
            || tx.description.contains("NOPA")
            || tx.description.contains("TARTINE")
        {
            card_patches.push(tag_patch(&tx.id, "2026-07-02", &["Dining"]));
        }
        if tx.description.contains("NETFLIX") {
            card_patches.push(tag_patch(&tx.id, "2026-07-02", &["Subscriptions"]));
        }
    }
    // Explicit annotation-level ignore via the first-class flag.
    card_patches.push(TransactionAnnotationPatch {
        transaction_id: reimbursed_id,
        timestamp: ts("2026-07-02"),
        description: None,
        note: None,
        tags: None,
        subtags: None,
        effective_date: None,
        ignore_spending: Some(Some(true)),
    });
    // Effective-date override: June travel booked in July.
    card_patches.push(TransactionAnnotationPatch {
        transaction_id: late_charge_id,
        timestamp: ts("2026-07-03"),
        description: None,
        note: None,
        tags: Some(Some(vec!["Travel".to_string()])),
        subtags: None,
        effective_date: Some(Some("2026-06-15".parse::<NaiveDate>()?)),
        ignore_spending: None,
    });
    storage
        .append_transaction_annotation_patches(&card.id, &card_patches)
        .await?;

    println!(
        "Seeded demo data at {} (config: {})",
        data_dir.display(),
        target.join("keepbook.toml").display()
    );
    Ok(())
}

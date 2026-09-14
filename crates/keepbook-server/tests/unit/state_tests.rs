use super::*;

// Shared with the other keepbook-server test modules; each one loads its own
// copy of the file.
#[allow(clippy::duplicate_mod)]
#[path = "test_support.rs"]
mod test_support;

use test_support::{remove_test_config, unique_test_config_path, write_test_config};

#[test]
fn account_portfolio_override_query_decodes_json_param() -> Result<()> {
    let encoded_overrides = serde_json::to_string(&serde_json::json!([
        {
            "account_id": "checking",
            "exclude_from_portfolio": false
        },
        {
            "account_id": "brokerage",
            "exclude_from_portfolio": true
        },
        {
            "account_id": "ira",
            "exclude_from_portfolio": true
        }
    ]))?;
    let encoded_query = serde_urlencoded::to_string([
        ("granularity", "weekly"),
        ("account_portfolio_overrides", encoded_overrides.as_str()),
    ])?;
    let query = serde_urlencoded::from_str::<HistoryQuery>(&encoded_query)?;

    assert_eq!(
        query.account_portfolio_overrides.as_deref(),
        Some(encoded_overrides.as_str())
    );

    let overrides = account_portfolio_exclusion_overrides(&query.account_portfolio_overrides)?;
    assert_eq!(overrides.get("checking"), Some(&false));
    assert_eq!(overrides.get("brokerage"), Some(&true));
    assert_eq!(overrides.get("ira"), Some(&true));
    Ok(())
}

#[cfg(feature = "http")]
#[tokio::test]
async fn portfolio_assets_returns_breakdown_and_respects_account_overrides() -> Result<()> {
    use chrono::TimeZone;
    use keepbook::models::AssetBalance;

    let config_path = unique_test_config_path("portfolio-assets");
    write_test_config(
        &config_path,
        "data_dir = \".\"\nreporting_currency = \"USD\"\n",
    )?;
    let data_dir = config_path
        .parent()
        .expect("config should have a parent")
        .to_path_buf();
    let storage = JsonFileStorage::new(&data_dir);

    let connection = keepbook::models::Connection::new(ConnectionConfig {
        name: "Bank".to_string(),
        synchronizer: "manual".to_string(),
        credentials: None,
        balance_staleness: None,
    });
    storage
        .save_connection_config(connection.id(), &connection.config)
        .await?;
    storage.save_connection(&connection).await?;

    let checking = Account::new("Checking", connection.id().clone());
    let savings = Account::new("Savings", connection.id().clone());
    storage.save_account(&checking).await?;
    storage.save_account(&savings).await?;

    let timestamp = Utc.with_ymd_and_hms(2024, 1, 15, 10, 0, 0).unwrap();
    storage
        .append_balance_snapshot(
            &checking.id,
            &BalanceSnapshot::new(
                timestamp,
                vec![AssetBalance::new(Asset::currency("USD"), "1000")],
            ),
        )
        .await?;
    storage
        .append_balance_snapshot(
            &savings.id,
            &BalanceSnapshot::new(
                timestamp,
                vec![AssetBalance::new(Asset::currency("USD"), "500")],
            ),
        )
        .await?;

    let state = ApiState::load(&config_path)?;

    let output = state
        .portfolio_assets(AssetsQuery {
            date: Some("2024-06-15".to_string()),
            account_portfolio_overrides: None,
            include_amount_changes: false,
        })
        .await?;
    assert_eq!(output.currency, "USD");
    assert_eq!(output.total_value, "1500");
    assert_eq!(output.assets.len(), 1);
    let entry = &output.assets[0];
    assert_eq!(entry.asset_id, "currency/USD");
    assert!(!entry.liability);
    assert_eq!(entry.total_amount, "1500");
    assert_eq!(entry.value_in_base.as_deref(), Some("1500"));
    assert_eq!(entry.holdings.len(), 2);
    assert!(entry
        .holdings
        .iter()
        .all(|holding| holding.connection_name.as_deref() == Some("Bank")));

    let overrides = serde_json::json!([
        {
            "account_id": savings.id.as_str(),
            "exclude_from_portfolio": true
        }
    ])
    .to_string();
    let output = state
        .portfolio_assets(AssetsQuery {
            date: Some("2024-06-15".to_string()),
            account_portfolio_overrides: Some(overrides),
            include_amount_changes: false,
        })
        .await?;
    assert_eq!(output.total_value, "1000");
    assert_eq!(output.assets.len(), 1);
    assert_eq!(output.assets[0].holdings.len(), 1);
    assert_eq!(
        output.assets[0].holdings[0].account_id,
        checking.id.to_string()
    );

    remove_test_config(config_path);
    Ok(())
}

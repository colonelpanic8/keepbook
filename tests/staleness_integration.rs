//! Integration tests for staleness configuration.

use std::time::Duration;

#[test]
fn test_parse_config_with_refresh() {
    let toml = r#"
data_dir = "data"

[refresh]
balance_staleness = "7d"
price_staleness = "1h"
"#;

    let config: keepbook::config::Config = toml::from_str(toml).unwrap();
    assert_eq!(config.refresh.balance_staleness, Duration::from_secs(7 * 24 * 60 * 60));
    assert_eq!(config.refresh.price_staleness, Duration::from_secs(60 * 60));
}

#[test]
fn test_connection_config_with_staleness() {
    let toml = r#"
name = "Test"
synchronizer = "manual"
balance_staleness = "3d"
"#;

    let config: keepbook::models::ConnectionConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.balance_staleness, Some(Duration::from_secs(3 * 24 * 60 * 60)));
}

#[test]
fn test_config_default_refresh_values() {
    // When refresh section is missing, defaults should be used
    let toml = r#"
data_dir = "data"
"#;

    let config: keepbook::config::Config = toml::from_str(toml).unwrap();
    // Default balance_staleness is 14 days
    assert_eq!(config.refresh.balance_staleness, Duration::from_secs(14 * 24 * 60 * 60));
    // Default price_staleness is 24 hours
    assert_eq!(config.refresh.price_staleness, Duration::from_secs(24 * 60 * 60));
}

#[test]
fn test_config_partial_refresh_values() {
    // When only some refresh values are specified, others use defaults
    let toml = r#"
data_dir = "data"

[refresh]
balance_staleness = "7d"
"#;

    let config: keepbook::config::Config = toml::from_str(toml).unwrap();
    assert_eq!(config.refresh.balance_staleness, Duration::from_secs(7 * 24 * 60 * 60));
    // price_staleness should use default (24 hours)
    assert_eq!(config.refresh.price_staleness, Duration::from_secs(24 * 60 * 60));
}

#[test]
fn test_connection_config_without_staleness() {
    let toml = r#"
name = "Test"
synchronizer = "manual"
"#;

    let config: keepbook::models::ConnectionConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.balance_staleness, None);
}

#[test]
fn test_account_config_with_staleness() {
    let toml = r#"
balance_staleness = "2d"
"#;

    let config: keepbook::models::AccountConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.balance_staleness, Some(Duration::from_secs(2 * 24 * 60 * 60)));
}

#[test]
fn test_account_config_empty() {
    // Empty account config should have None for balance_staleness
    let toml = "";

    let config: keepbook::models::AccountConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.balance_staleness, None);
}

#[test]
fn test_duration_formats() {
    // Test various duration formats in config

    // Hours
    let toml = r#"
data_dir = "data"

[refresh]
balance_staleness = "12h"
price_staleness = "30m"
"#;
    let config: keepbook::config::Config = toml::from_str(toml).unwrap();
    assert_eq!(config.refresh.balance_staleness, Duration::from_secs(12 * 60 * 60));
    assert_eq!(config.refresh.price_staleness, Duration::from_secs(30 * 60));
}

#[test]
fn test_duration_seconds_format() {
    let toml = r#"
name = "Test"
synchronizer = "manual"
balance_staleness = "3600s"
"#;

    let config: keepbook::models::ConnectionConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.balance_staleness, Some(Duration::from_secs(3600)));
}

#[test]
fn test_duration_days_for_two_weeks() {
    // Weeks are not directly supported, use days instead (14d = 2 weeks)
    let toml = r#"
name = "Test"
synchronizer = "manual"
balance_staleness = "14d"
"#;

    let config: keepbook::models::ConnectionConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.balance_staleness, Some(Duration::from_secs(14 * 24 * 60 * 60)));
}

#[test]
fn test_invalid_duration_format_rejected() {
    // Ensure invalid formats are properly rejected
    let toml = r#"
name = "Test"
synchronizer = "manual"
balance_staleness = "2w"
"#;

    let result: Result<keepbook::models::ConnectionConfig, _> = toml::from_str(toml);
    assert!(result.is_err(), "Invalid duration format 'w' should be rejected");
}

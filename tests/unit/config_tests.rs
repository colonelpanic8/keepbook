use super::*;
use std::io::Write;
use tempfile::TempDir;

#[test]
fn test_default_data_dir_is_config_dir() {
    let config = Config::default();
    let config_dir = Path::new("/home/user/finances");
    assert_eq!(
        config.resolve_data_dir(config_dir),
        PathBuf::from("/home/user/finances")
    );
}

#[test]
fn test_relative_data_dir() {
    let config = Config {
        data_dir: Some(PathBuf::from("data")),
        ..Default::default()
    };
    let config_dir = Path::new("/home/user/finances");
    assert_eq!(
        config.resolve_data_dir(config_dir),
        PathBuf::from("/home/user/finances/data")
    );
}

#[test]
fn test_absolute_data_dir() {
    let config = Config {
        data_dir: Some(PathBuf::from("/var/keepbook/data")),
        ..Default::default()
    };
    let config_dir = Path::new("/home/user/finances");
    assert_eq!(
        config.resolve_data_dir(config_dir),
        PathBuf::from("/var/keepbook/data")
    );
}

#[test]
fn test_tilde_data_dir_expands_to_home() {
    let Some(home_dir) = dirs::home_dir() else {
        return;
    };

    let config = Config {
        data_dir: Some(PathBuf::from("~/keepbook-data")),
        ..Default::default()
    };
    let config_dir = Path::new("/home/user/finances");
    assert_eq!(
        config.resolve_data_dir(config_dir),
        home_dir.join("keepbook-data")
    );
}

#[test]
fn test_expand_tilde_path() {
    let Some(home_dir) = dirs::home_dir() else {
        return;
    };

    assert_eq!(expand_tilde_path(Path::new("~")), home_dir);
    assert_eq!(
        expand_tilde_path(Path::new("~/keepbook.toml")),
        dirs::home_dir().unwrap().join("keepbook.toml")
    );
    assert_eq!(
        expand_tilde_path(Path::new("~other/keepbook.toml")),
        PathBuf::from("~other/keepbook.toml")
    );
}

#[test]
fn test_load_config() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "data_dir = \"./my-data\"")?;

    let config = Config::load(&config_path)?;
    assert_eq!(config.data_dir, Some(PathBuf::from("./my-data")));

    Ok(())
}

#[test]
fn test_load_empty_config() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    std::fs::File::create(&config_path)?;

    let config = Config::load(&config_path)?;
    assert_eq!(config.data_dir, None);

    Ok(())
}

#[test]
fn test_load_refresh_config() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[refresh]")?;
    writeln!(file, "balance_staleness = \"7d\"")?;
    writeln!(file, "price_staleness = \"1h\"")?;

    let config = Config::load(&config_path)?;
    assert_eq!(
        config.refresh.balance_staleness,
        std::time::Duration::from_secs(7 * 24 * 60 * 60)
    );
    assert_eq!(
        config.refresh.price_staleness,
        std::time::Duration::from_secs(60 * 60)
    );

    Ok(())
}

#[test]
fn test_load_git_config() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[git]")?;
    writeln!(file, "auto_commit = true")?;
    writeln!(file, "auto_push = true")?;
    writeln!(file, "pull_before_edit = true")?;
    writeln!(file, "push_after_sync = true")?;
    writeln!(file, "merge_master_before_command = true")?;

    let config = Config::load(&config_path)?;
    assert!(config.git.auto_commit);
    assert!(config.git.auto_push);
    assert!(config.git.pull_before_edit);
    assert!(config.git.push_after_sync);
    assert!(config.git.merge_master_before_command);

    Ok(())
}

#[test]
fn test_load_git_config_defaults_auto_push_to_auto_commit() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[git]")?;
    writeln!(file, "auto_commit = true")?;

    let config = Config::load(&config_path)?;
    assert!(config.git.auto_commit);
    assert!(config.git.auto_push);
    assert!(!config.git.pull_before_edit);
    assert!(!config.git.push_after_sync);
    assert!(!config.git.merge_master_before_command);

    Ok(())
}

#[test]
fn test_load_git_config_allows_disabling_auto_push() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[git]")?;
    writeln!(file, "auto_commit = true")?;
    writeln!(file, "auto_push = false")?;

    let config = Config::load(&config_path)?;
    assert!(config.git.auto_commit);
    assert!(!config.git.auto_push);

    Ok(())
}

#[test]
fn test_load_display_currency_decimals() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[display]")?;
    writeln!(file, "currency_decimals = 2")?;

    let config = Config::load(&config_path)?;
    assert_eq!(config.display.currency_decimals, Some(2));

    Ok(())
}

#[test]
fn test_load_display_currency_formatting_options() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[display]")?;
    writeln!(file, "currency_grouping = true")?;
    writeln!(file, "currency_symbol = \"$\"")?;
    writeln!(file, "currency_fixed_decimals = true")?;

    let config = Config::load(&config_path)?;
    assert!(config.display.currency_grouping);
    assert_eq!(config.display.currency_symbol.as_deref(), Some("$"));
    assert!(config.display.currency_fixed_decimals);

    Ok(())
}

#[test]
fn test_load_tray_config() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[tray]")?;
    writeln!(file, "start_minimized = true")?;
    writeln!(file, "history_points = 5")?;
    writeln!(file, "spending_windows_days = [3, 14, 60]")?;

    let config = Config::load(&config_path)?;
    assert!(config.tray.start_minimized);
    assert_eq!(config.tray.history_points, 5);
    assert_eq!(config.tray.spending_windows_days, vec![3, 14, 60]);

    Ok(())
}

#[test]
fn test_load_history_defaults_config() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[history]")?;
    writeln!(file, "portfolio_granularity = \"weekly\"")?;
    writeln!(file, "change_points_granularity = \"daily\"")?;
    writeln!(file, "include_prices = false")?;
    writeln!(file, "graph_range = \"2y\"")?;
    writeln!(file, "graph_granularity = \"monthly\"")?;

    let config = Config::load(&config_path)?;
    assert_eq!(config.history.portfolio_granularity, "weekly");
    assert_eq!(config.history.change_points_granularity, "daily");
    assert!(!config.history.include_prices);
    assert_eq!(config.history.graph_range, "2y");
    assert_eq!(config.history.graph_granularity, "monthly");

    Ok(())
}

#[test]
fn test_default_refresh_config() {
    let config = Config::default();
    assert_eq!(
        config.refresh.balance_staleness,
        std::time::Duration::from_secs(14 * 24 * 60 * 60)
    );
    assert_eq!(
        config.refresh.price_staleness,
        std::time::Duration::from_secs(24 * 60 * 60)
    );
}

#[test]
fn test_default_git_config() {
    let config = Config::default();
    assert!(!config.git.auto_commit);
    assert!(!config.git.auto_push);
    assert!(!config.git.pull_before_edit);
    assert!(!config.git.push_after_sync);
    assert!(!config.git.merge_master_before_command);
}

#[test]
fn test_default_tray_config() {
    let config = Config::default();
    assert!(!config.tray.start_minimized);
    assert_eq!(config.tray.history_points, 17);
    assert_eq!(
        config.tray.history_spec,
        vec![
            "last 4 days".to_string(),
            "1 week ago".to_string(),
            "2 weeks ago".to_string(),
            "last 12 months".to_string()
        ]
    );
    assert_eq!(config.tray.spending_windows_days, vec![7, 30, 90, 365]);
}

#[test]
fn test_default_history_config() {
    let config = Config::default();
    assert_eq!(config.history.portfolio_granularity, "daily");
    assert_eq!(config.history.change_points_granularity, "none");
    assert!(config.history.include_prices);
    assert_eq!(config.history.graph_range, "1y");
    assert_eq!(config.history.graph_granularity, "weekly");
}

#[test]
fn test_default_spending_config() {
    let config = Config::default();
    assert!(config.spending.ignore_accounts.is_empty());
    assert!(config.spending.ignore_connections.is_empty());
    assert!(config.spending.ignore_tags.is_empty());
    assert!(config.tags.aliases.is_empty());
    assert!(config.tags.parents.is_empty());
}

#[test]
fn test_default_ignore_config() {
    let config = Config::default();
    assert!(config.ignore.transaction_rules.is_empty());
}

#[test]
fn test_absolute_from_current_dir_preserves_absolute_paths() {
    let path = PathBuf::from("/tmp/keepbook.toml");
    assert_eq!(absolute_from_current_dir(path.clone()), path);
}

#[test]
fn test_absolute_from_current_dir_resolves_relative_paths() {
    let path = absolute_from_current_dir(PathBuf::from("keepbook.toml"));
    assert!(path.is_absolute());
    assert!(path.ends_with("keepbook.toml"));
}

#[test]
fn test_absolute_from_current_dir_expands_tilde_paths() {
    let Some(home_dir) = dirs::home_dir() else {
        return;
    };

    let path = absolute_from_current_dir(PathBuf::from("~/keepbook.toml"));
    assert_eq!(path, home_dir.join("keepbook.toml"));
}

#[test]
fn test_load_spending_config() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[spending]")?;
    writeln!(file, "ignore_accounts = [\"Individual\", \"acct-1\"]")?;
    writeln!(file, "ignore_connections = [\"Schwab\"]")?;
    writeln!(file, "ignore_tags = [\"brokerage\"]")?;

    let config = Config::load(&config_path)?;
    assert_eq!(config.spending.ignore_accounts.len(), 2);
    assert_eq!(config.spending.ignore_connections, vec!["Schwab"]);
    assert_eq!(config.spending.ignore_tags, vec!["brokerage"]);

    Ok(())
}

#[test]
fn test_load_tag_hierarchy_config() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[tags.aliases]")?;
    writeln!(file, "\"Food And Drink\" = \"Food\"")?;
    writeln!(file, "[tags.parents]")?;
    writeln!(file, "\"Groceries\" = [\"Food\"]")?;
    writeln!(file, "\"Supermarket\" = [\"Groceries\"]")?;

    let config = Config::load(&config_path)?;
    assert_eq!(
        config
            .tags
            .aliases
            .get("Food And Drink")
            .map(String::as_str),
        Some("Food")
    );
    assert_eq!(
        config.tags.root_tags_for("Supermarket"),
        vec!["Food".to_string()]
    );
    assert_eq!(
        config.tags.canonical_provider_label("FOOD_AND_DRINK"),
        Some("Food".to_string())
    );

    Ok(())
}

#[test]
fn test_tag_alias_can_point_to_root_without_parent_entry() {
    let config = TagsConfig {
        aliases: HashMap::from([("FOOD_AND_DRINK".to_string(), "Food".to_string())]),
        parents: HashMap::new(),
    };

    assert_eq!(
        config.canonical_provider_label("food and drink"),
        Some("Food".to_string())
    );
    assert_eq!(config.root_tags_for("Food"), vec!["Food".to_string()]);
}

#[test]
fn test_load_portfolio_latent_capital_gains_tax_config() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[portfolio.latent_capital_gains_tax]")?;
    writeln!(file, "enabled = true")?;
    writeln!(file, "rate = 0.23")?;
    writeln!(file, "account_name = \"Estimated Tax Liability\"")?;

    let config = Config::load(&config_path)?;
    assert!(config.portfolio.latent_capital_gains_tax.enabled);
    assert_eq!(config.portfolio.latent_capital_gains_tax.rate, Some(0.23));
    assert_eq!(
        config.portfolio.latent_capital_gains_tax.account_name,
        "Estimated Tax Liability"
    );

    Ok(())
}

#[test]
fn test_load_ignore_transaction_rules_config() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "[ignore]")?;
    writeln!(file, "[[ignore.transaction_rules]]")?;
    writeln!(file, "account_name = \"(?i)^Investor Checking$\"")?;
    writeln!(
        file,
        "description = \"(?i)credit\\\\s+crd\\\\s+(?:e?pay|autopay)\""
    )?;
    writeln!(file, "synchronizer = \"(?i)^schwab$\"")?;

    let config = Config::load(&config_path)?;
    assert_eq!(config.ignore.transaction_rules.len(), 1);
    let rule = &config.ignore.transaction_rules[0];
    assert_eq!(
        rule.account_name.as_deref(),
        Some("(?i)^Investor Checking$")
    );
    assert_eq!(
        rule.description.as_deref(),
        Some("(?i)credit\\s+crd\\s+(?:e?pay|autopay)")
    );
    assert_eq!(rule.synchronizer.as_deref(), Some("(?i)^schwab$"));

    Ok(())
}

#[test]
fn test_config_load_or_default_missing_file() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("missing.toml");

    let config = Config::load_or_default(&config_path)?;
    assert_eq!(config.data_dir, None);
    assert_eq!(config.reporting_currency, "USD");

    Ok(())
}

#[test]
fn test_resolved_config_load_or_default_missing_file() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let resolved = ResolvedConfig::load_or_default(&config_path)?;
    assert_eq!(resolved.data_dir, dir.path());
    assert_eq!(resolved.reporting_currency, "USD");

    Ok(())
}

#[test]
fn test_resolved_config_resolves_relative_data_dir() -> Result<()> {
    let dir = TempDir::new()?;
    let config_path = dir.path().join("keepbook.toml");

    let mut file = std::fs::File::create(&config_path)?;
    writeln!(file, "data_dir = \"./data\"")?;

    let resolved = ResolvedConfig::load(&config_path)?;
    let expected_config_dir = config_path
        .canonicalize()?
        .parent()
        .context("Config file has no parent directory")?
        .to_path_buf();
    assert_eq!(resolved.data_dir, expected_config_dir.join("data"));

    Ok(())
}

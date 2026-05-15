use super::*;
use crate::market_data::source_config::{LoadedPriceSource, PriceSourceConfig, PriceSourceType};
use std::fs;
use std::io::Write;
use tempfile::TempDir;

#[tokio::test]
async fn build_equity_sources_missing_credentials_returns_error_not_panic() -> Result<()> {
    // PriceSourceConfig::load() validates this, but we still want the registry to be robust
    // against malformed state (hand-edits, older versions, etc.).
    let dir = TempDir::new()?;
    let mut registry = PriceSourceRegistry::new(dir.path());
    registry.loaded = vec![LoadedPriceSource {
        name: "bad-eodhd".to_string(),
        base_dir: dir.path().join("bad-eodhd"),
        config: PriceSourceConfig {
            source_type: PriceSourceType::Eodhd,
            enabled: true,
            priority: 1,
            credentials: None,
            config: None,
        },
    }];

    match registry.build_equity_sources().await {
        Ok(_) => anyhow::bail!("expected error for missing credentials"),
        Err(err) => {
            assert!(err.to_string().contains("requires credentials"));
        }
    }

    Ok(())
}

#[test]
fn test_load_empty_directory() -> Result<()> {
    let dir = TempDir::new()?;
    let mut registry = PriceSourceRegistry::new(dir.path());
    registry.load()?;
    assert!(registry.sources().is_empty());
    Ok(())
}

#[test]
fn test_load_sources() -> Result<()> {
    let dir = TempDir::new()?;
    let sources_dir = dir.path().join("price_sources");

    // Create coingecko source (no credentials needed)
    let cg_dir = sources_dir.join("coingecko");
    fs::create_dir_all(&cg_dir)?;
    let mut file = fs::File::create(cg_dir.join("source.toml"))?;
    writeln!(file, r#"type = "coingecko""#)?;
    writeln!(file, r#"priority = 20"#)?;

    // Create frankfurter source
    let ff_dir = sources_dir.join("frankfurter");
    fs::create_dir_all(&ff_dir)?;
    let mut file = fs::File::create(ff_dir.join("source.toml"))?;
    writeln!(file, r#"type = "frankfurter""#)?;
    writeln!(file, r#"priority = 10"#)?;

    let mut registry = PriceSourceRegistry::new(dir.path());
    registry.load()?;

    assert_eq!(registry.sources().len(), 2);
    // Should be sorted by priority
    assert_eq!(registry.sources()[0].config.priority, 10);
    assert_eq!(registry.sources()[1].config.priority, 20);

    Ok(())
}

#[test]
fn test_disabled_source_not_loaded() -> Result<()> {
    let dir = TempDir::new()?;
    let sources_dir = dir.path().join("price_sources");

    let cg_dir = sources_dir.join("coingecko");
    fs::create_dir_all(&cg_dir)?;
    let mut file = fs::File::create(cg_dir.join("source.toml"))?;
    writeln!(file, r#"type = "coingecko""#)?;
    writeln!(file, r#"enabled = false"#)?;

    let mut registry = PriceSourceRegistry::new(dir.path());
    registry.load()?;

    assert!(registry.sources().is_empty());

    Ok(())
}

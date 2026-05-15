use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_parse_eodhd_config() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    writeln!(
        file,
        r#"
type = "eodhd"
enabled = true
priority = 10

[credentials]
backend = "pass"
path = "keepbook/eodhd-api-key"
"#
    )?;

    let config = PriceSourceConfig::load(file.path())?;
    assert_eq!(config.source_type, PriceSourceType::Eodhd);
    assert!(config.enabled);
    assert_eq!(config.priority, 10);
    assert!(config.credentials.is_some());

    Ok(())
}

#[test]
fn test_parse_coingecko_config() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    writeln!(
        file,
        r#"
type = "coingecko"
priority = 20
"#
    )?;

    let config = PriceSourceConfig::load(file.path())?;
    assert_eq!(config.source_type, PriceSourceType::Coingecko);
    assert!(config.enabled); // default
    assert_eq!(config.priority, 20);
    assert!(config.credentials.is_none()); // not required

    Ok(())
}

#[test]
fn test_missing_credentials_for_eodhd() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(
        file,
        r#"
type = "eodhd"
"#
    )
    .unwrap();

    let result = PriceSourceConfig::load(file.path());
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("requires credentials"));
}

#[test]
fn test_source_type_requires_credentials() {
    assert!(PriceSourceType::Eodhd.requires_credentials());
    assert!(PriceSourceType::TwelveData.requires_credentials());
    assert!(PriceSourceType::AlphaVantage.requires_credentials());
    assert!(PriceSourceType::Marketstack.requires_credentials());
    assert!(!PriceSourceType::Coingecko.requires_credentials());
    assert!(!PriceSourceType::Frankfurter.requires_credentials());
}

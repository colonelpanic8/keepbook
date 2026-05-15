use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_parse_pass_config() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    writeln!(
        file,
        r#"
backend = "pass"
path = "finance/coinbase-api"

[fields]
key_name = "key-name"
private_key = "private-key"
"#
    )?;

    let config = CredentialConfig::load(file.path())?;

    match config {
        CredentialConfig::Pass { config } => {
            assert_eq!(config.path, "finance/coinbase-api");
            assert_eq!(config.fields.get("key_name"), Some(&"key-name".to_string()));
            assert_eq!(
                config.fields.get("private_key"),
                Some(&"private-key".to_string())
            );
        }
        _ => panic!("expected pass credential config"),
    }

    Ok(())
}

#[test]
fn test_parse_minimal_pass_config() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    writeln!(
        file,
        r#"
backend = "pass"
path = "my-api-key"
"#
    )?;

    let config = CredentialConfig::load(file.path())?;

    match config {
        CredentialConfig::Pass { config } => {
            assert_eq!(config.path, "my-api-key");
            assert!(config.fields.is_empty());
        }
        _ => panic!("expected pass credential config"),
    }

    Ok(())
}

#[test]
fn test_parse_env_config() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    writeln!(
        file,
        r#"
backend = "env"
prefix = "KEEPBOOK_COINBASE_"

[fields]
key_name = "KEEPBOOK_COINBASE_KEY_NAME"
"#
    )?;

    let config = CredentialConfig::load(file.path())?;

    match config {
        CredentialConfig::Env { config } => {
            assert_eq!(config.prefix.as_deref(), Some("KEEPBOOK_COINBASE_"));
            assert_eq!(
                config.fields.get("key_name"),
                Some(&"KEEPBOOK_COINBASE_KEY_NAME".to_string())
            );
        }
        _ => panic!("expected env credential config"),
    }

    Ok(())
}

#[test]
fn test_parse_age_config() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    writeln!(
        file,
        r#"
backend = "age"
path = "credentials/coinbase.age"
identity_path = ".ssh/keepbook_sync_key"

[fields]
key_name = "key-name"
"#
    )?;

    let config = CredentialConfig::load(file.path())?;

    match config {
        CredentialConfig::Age { config } => {
            assert_eq!(config.path, "credentials/coinbase.age");
            assert_eq!(
                config.identity_path.as_deref(),
                Some(".ssh/keepbook_sync_key")
            );
            assert_eq!(config.fields.get("key_name"), Some(&"key-name".to_string()));
        }
        _ => panic!("expected age credential config"),
    }

    Ok(())
}

// tests/portfolio_integration.rs
use anyhow::Result;
use std::process::Command;
use tempfile::TempDir;

#[test]
fn portfolio_snapshot_empty() -> Result<()> {
    let temp = TempDir::new()?;
    let config_path = temp.path().join("keepbook.toml");
    std::fs::write(
        &config_path,
        format!(
            r#"
data_dir = "{}"
reporting_currency = "USD"
"#,
            temp.path().display()
        ),
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_keepbook"))
        .args([
            "--config",
            config_path.to_str().unwrap(),
            "portfolio",
            "snapshot",
            "--date",
            "2024-06-15",
        ])
        .output()?;

    assert!(output.status.success(), "Command failed: {output:?}");

    let stdout = String::from_utf8(output.stdout)?;
    let json: serde_json::Value = serde_json::from_str(&stdout)?;

    let contract_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("contracts")
        .join("portfolio_snapshot_empty.json");
    let contract = std::fs::read_to_string(contract_path)?;
    let expected: serde_json::Value = serde_json::from_str(&contract)?;

    assert_eq!(json, expected);

    Ok(())
}

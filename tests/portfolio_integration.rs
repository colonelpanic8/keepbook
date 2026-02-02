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
        .args(["--config", config_path.to_str().unwrap(), "portfolio", "snapshot"])
        .output()?;

    assert!(output.status.success(), "Command failed: {output:?}");

    let stdout = String::from_utf8(output.stdout)?;
    let json: serde_json::Value = serde_json::from_str(&stdout)?;

    assert_eq!(json["total_value"], "0");
    assert_eq!(json["currency"], "USD");

    Ok(())
}

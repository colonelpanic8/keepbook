use std::path::Path;

use anyhow::Result;
use keepbook::app;
use keepbook::config::ResolvedConfig;

pub fn run(config_path: &Path, config: &ResolvedConfig) -> Result<()> {
    let output = app::config_output(config_path, config);
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

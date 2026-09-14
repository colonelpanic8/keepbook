use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use keepbook::config::{default_config_path, ResolvedConfig, WindowDecorationsConfig};
use toml_edit::{value, DocumentMut, Item, Table};

use crate::dto::ApplicationSettingsInput;

pub(crate) fn load_api_config(config_path: &Path) -> Result<ResolvedConfig> {
    ResolvedConfig::load_or_default(config_path)
}

pub(crate) fn load_config_doc(config_path: &Path) -> Result<DocumentMut> {
    if config_path.exists() {
        let content = std::fs::read_to_string(config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?;
        content
            .parse::<DocumentMut>()
            .with_context(|| format!("failed to parse {}", config_path.display()))
    } else {
        Ok(DocumentMut::new())
    }
}

pub(crate) fn write_application_settings(
    config_path: &Path,
    input: &ApplicationSettingsInput,
) -> Result<()> {
    let window_decorations = WindowDecorationsConfig::from_str(&input.window_decorations)?;
    let mut doc = load_config_doc(config_path)?;
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if doc
        .get("tray")
        .is_none_or(|item| item.as_table_like().is_none())
    {
        doc.insert("tray", Item::Table(Table::new()));
    }
    doc["tray"]["start_minimized"] = value(input.start_minimized_to_tray);
    doc["tray"]["window_decorations"] = value(window_decorations.as_str());

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    Ok(())
}

pub(crate) fn non_empty(value: &str, default: &str) -> String {
    if value.is_empty() {
        default.to_string()
    } else {
        value.to_string()
    }
}

pub fn default_listen_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 8799))
}

pub fn default_server_config_path() -> PathBuf {
    default_config_path()
}

pub fn desktop_start_minimized_to_tray(config_path: impl AsRef<Path>) -> Result<bool> {
    if let Some(value) = std::env::var_os("KEEPBOOK_START_MINIMIZED_TO_TRAY") {
        return parse_bool_setting(&value.to_string_lossy()).with_context(|| {
            "invalid KEEPBOOK_START_MINIMIZED_TO_TRAY value; use true/false or 1/0"
        });
    }
    Ok(ResolvedConfig::load_or_default(config_path.as_ref())?
        .tray
        .start_minimized)
}

pub fn desktop_window_decorations(
    config_path: impl AsRef<Path>,
) -> Result<WindowDecorationsConfig> {
    Ok(ResolvedConfig::load_or_default(config_path.as_ref())?
        .tray
        .window_decorations)
}

fn parse_bool_setting(value: &str) -> Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => anyhow::bail!("expected a boolean value, got {value:?}"),
    }
}

#[cfg(test)]
#[path = "../tests/unit/settings_tests.rs"]
mod settings_tests;

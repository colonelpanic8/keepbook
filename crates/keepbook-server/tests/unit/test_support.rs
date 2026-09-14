//! Helpers shared by the keepbook-server unit test modules.
//!
//! Each module that mounts this file gets its own copy, so unused helpers are
//! expected.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

pub fn unique_test_config_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "keepbook-server-{name}-{}-{nanos}/keepbook.toml",
        std::process::id()
    ))
}

pub fn write_test_config(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    Ok(())
}

pub fn remove_test_config(path: PathBuf) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::remove_dir_all(parent);
    }
}

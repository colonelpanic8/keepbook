//! Session cache for storing transient authentication state.
//!
//! This module provides local-only storage for session tokens, cookies,
//! and other ephemeral authentication data that shouldn't be synced.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Session data for a connection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionData {
    /// Bearer token or similar auth token.
    #[serde(default)]
    pub token: Option<String>,

    /// Session cookies (name -> value).
    #[serde(default)]
    pub cookies: HashMap<String, String>,

    /// When the session was captured (Unix timestamp).
    #[serde(default)]
    pub captured_at: Option<i64>,

    /// Arbitrary key-value data for synchronizer-specific needs.
    #[serde(default)]
    pub data: HashMap<String, String>,
}

impl SessionData {
    /// Create a new empty session.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the bearer token.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Add a cookie.
    pub fn with_cookie(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.cookies.insert(name.into(), value.into());
        self
    }

    /// Format cookies as a Cookie header value.
    pub fn cookie_header(&self) -> String {
        self.cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// Cache for session data, stored locally (not synced).
pub struct SessionCache {
    cache_dir: PathBuf,
}

impl SessionCache {
    /// Create a new session cache.
    ///
    /// Uses `~/.cache/keepbook/sessions/` by default.
    pub fn new() -> Result<Self> {
        let cache_dir = dirs::cache_dir()
            .context("Could not find cache directory")?
            .join("keepbook")
            .join("sessions");

        std::fs::create_dir_all(&cache_dir)
            .with_context(|| format!("Failed to create session cache dir: {cache_dir:?}"))?;

        Ok(Self { cache_dir })
    }

    /// Create a session cache at a custom location.
    pub fn with_path(cache_dir: impl AsRef<Path>) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&cache_dir)
            .with_context(|| format!("Failed to create session cache dir: {cache_dir:?}"))?;
        Ok(Self { cache_dir })
    }

    fn session_file(&self, connection_id: &str) -> PathBuf {
        self.cache_dir.join(format!("{connection_id}.json"))
    }

    /// Load session data for a connection.
    pub fn get(&self, connection_id: &str) -> Result<Option<SessionData>> {
        let path = self.session_file(connection_id);
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read session file: {path:?}"))?;

        let session: SessionData = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse session file: {path:?}"))?;

        Ok(Some(session))
    }

    /// Save session data for a connection.
    pub fn set(&self, connection_id: &str, session: &SessionData) -> Result<()> {
        let path = self.session_file(connection_id);
        let content =
            serde_json::to_string_pretty(session).context("Failed to serialize session")?;

        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write session file: {path:?}"))?;

        Ok(())
    }

    /// Delete session data for a connection.
    pub fn delete(&self, connection_id: &str) -> Result<()> {
        let path = self.session_file(connection_id);
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("Failed to delete session file: {path:?}"))?;
        }
        Ok(())
    }
}

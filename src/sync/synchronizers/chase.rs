//! Chase synchronizer with browser-based download.
//!
//! This synchronizer opens a real browser so the user can complete
//! login + 2FA and download QFX files. It captures session cookies
//! for reuse but does not yet parse QFX into transactions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::browser::{
    SetDownloadBehaviorBehavior, SetDownloadBehaviorParams,
};
use chromiumoxide::cdp::browser_protocol::network::CookieParam;
use chrono::Utc;
use futures::StreamExt;

use crate::credentials::{SessionCache, SessionData};
use crate::models::{Connection, ConnectionStatus, LastSync, SyncStatus};
use crate::storage::Storage;
use crate::sync::{AuthStatus, InteractiveAuth, SyncResult, Synchronizer};

const CHASE_LOGIN_URL: &str = "https://www.chase.com";
const DOWNLOAD_TIMEOUT_SECS: u64 = 600;
const DOWNLOAD_IDLE_SECS: u64 = 30;

/// Chase synchronizer using browser automation for QFX downloads.
pub struct ChaseSynchronizer {
    connection_id: crate::models::Id,
    session_cache: SessionCache,
    download_root: PathBuf,
}

impl ChaseSynchronizer {
    /// Create a new Chase synchronizer for a connection using a default download directory.
    pub async fn from_connection<S: Storage + ?Sized>(
        connection: &Connection,
        _storage: &S,
    ) -> Result<Self> {
        let download_root = default_download_root()?;
        Ok(Self {
            connection_id: connection.id().clone(),
            session_cache: SessionCache::new()?,
            download_root,
        })
    }

    /// Create a new Chase synchronizer with downloads rooted in `base_dir`.
    pub async fn from_connection_with_download_dir<S: Storage + ?Sized>(
        connection: &Connection,
        _storage: &S,
        base_dir: &Path,
    ) -> Result<Self> {
        Ok(Self {
            connection_id: connection.id().clone(),
            session_cache: SessionCache::new()?,
            download_root: base_dir.join("downloads").join("chase"),
        })
    }

    /// Create a synchronizer using an explicit session cache (useful for tests).
    pub fn with_session_cache(connection: &Connection, session_cache: SessionCache) -> Result<Self> {
        let download_root = default_download_root()?;
        Ok(Self {
            connection_id: connection.id().clone(),
            session_cache,
            download_root,
        })
    }

    fn session_key(&self) -> String {
        self.connection_id.to_string()
    }

    fn get_session(&self) -> Result<Option<SessionData>> {
        self.session_cache.get(&self.session_key())
    }

    fn ensure_download_dir(&self) -> Result<PathBuf> {
        let dir = self.download_root.join(self.connection_id.to_string());
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create download dir: {}", dir.display()))?;
        Ok(dir)
    }

    fn ensure_profile_dir(&self) -> Result<PathBuf> {
        let dir = self
            .download_root
            .join("profiles")
            .join(self.connection_id.to_string());
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create profile dir: {}", dir.display()))?;
        Ok(dir)
    }

    async fn sync_internal(&self, connection: &mut Connection) -> Result<SyncResult> {
        let download_dir = self.ensure_download_dir()?;

        let profile_dir = self.ensure_profile_dir()?;
        let (browser, mut handler) = launch_browser(&profile_dir).await?;
        let handler_task = tokio::spawn(async move { while (handler.next().await).is_some() {} });

        let page = browser.new_page("about:blank").await?;

        setup_download_handling(&page, &download_dir).await?;

        // Navigate first so we can set cookies on a valid URL.
        page.goto(CHASE_LOGIN_URL).await?;

        // Apply cached cookies if present (best-effort).
        if let Some(session) = self.get_session()? {
            if !session.cookies.is_empty() {
                apply_cookies(&page, &session).await.ok();
                // Reload to apply cookies.
                page.goto(CHASE_LOGIN_URL).await.ok();
            }
        }

        println!("\n========================================");
        println!("Chase download instructions:");
        println!("1. Log in (complete 2FA as needed)");
        println!("2. Navigate to an account");
        println!("3. Click 'Download account activity'");
        println!("4. Choose a date range and QFX format");
        println!("5. Click Download");
        println!("========================================\n");
        println!("Waiting for downloads in: {}", download_dir.display());
        println!("Will stop after {} seconds of inactivity.", DOWNLOAD_IDLE_SECS);

        let downloads = watch_for_downloads(
            &download_dir,
            Duration::from_secs(DOWNLOAD_TIMEOUT_SECS),
            Duration::from_secs(DOWNLOAD_IDLE_SECS),
        )
        .await?;

        if downloads.is_empty() {
            anyhow::bail!("No downloads detected. Try again and make sure the QFX download completes.");
        }

        // Refresh session cookies for reuse.
        if let Ok(cookies) = page.get_cookies().await {
            let mut cookie_map = HashMap::new();
            for cookie in cookies {
                cookie_map.insert(cookie.name.clone(), cookie.value.clone());
            }
            let session = SessionData {
                token: None,
                cookies: cookie_map,
                captured_at: Some(Utc::now().timestamp()),
                data: HashMap::new(),
            };
            let _ = self.session_cache.set(&self.session_key(), &session);
        }

        // Update connection state.
        connection.state.last_sync = Some(LastSync {
            at: Utc::now(),
            status: SyncStatus::Success,
            error: None,
        });
        connection.state.status = ConnectionStatus::Active;

        // Record downloads for later parsing.
        let download_list: Vec<String> = downloads
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        let mut data = connection
            .state
            .synchronizer_data
            .as_object()
            .cloned()
            .unwrap_or_default();
        data.insert(
            "download_dir".to_string(),
            serde_json::Value::String(download_dir.display().to_string()),
        );
        data.insert(
            "downloads".to_string(),
            serde_json::json!(download_list),
        );
        data.insert(
            "downloaded_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
        connection.state.synchronizer_data = serde_json::Value::Object(data);

        drop(browser);
        handler_task.abort();

        Ok(SyncResult {
            connection: connection.clone(),
            accounts: Vec::new(),
            balances: Vec::new(),
            transactions: Vec::new(),
        })
    }
}

#[async_trait::async_trait]
impl Synchronizer for ChaseSynchronizer {
    fn name(&self) -> &str {
        "chase"
    }

    async fn sync(&self, connection: &mut Connection, _storage: &dyn Storage) -> Result<SyncResult> {
        self.sync_internal(connection).await
    }

    fn interactive(&mut self) -> Option<&mut dyn InteractiveAuth> {
        Some(self)
    }
}

impl ChaseSynchronizer {
    /// Sync with storage access for future account lookups.
    pub async fn sync_with_storage<S: Storage>(
        &self,
        connection: &mut Connection,
        _storage: &S,
    ) -> Result<SyncResult> {
        self.sync_internal(connection).await
    }
}

#[async_trait::async_trait]
impl InteractiveAuth for ChaseSynchronizer {
    async fn check_auth(&self) -> Result<AuthStatus> {
        match self.get_session()? {
            None => Ok(AuthStatus::Missing),
            Some(session) => {
                if session.cookies.is_empty() {
                    return Ok(AuthStatus::Missing);
                }

                if let Some(captured_at) = session.captured_at {
                    let age_secs = Utc::now().timestamp() - captured_at;
                    if age_secs > 24 * 60 * 60 {
                        return Ok(AuthStatus::Expired {
                            reason: format!("Session is {} hours old", age_secs / 3600),
                        });
                    }
                }

                Ok(AuthStatus::Valid)
            }
        }
    }

    async fn login(&mut self) -> Result<()> {
        let profile_dir = self.ensure_profile_dir()?;
        let (browser, mut handler) = launch_browser(&profile_dir).await?;
        let handler_task = tokio::spawn(async move { while (handler.next().await).is_some() {} });

        let page = browser.new_page("about:blank").await?;

        page.goto(CHASE_LOGIN_URL).await?;

        println!("\n========================================");
        println!("Complete Chase login in the browser.");
        println!("When finished, return here and press Enter.");
        println!("========================================\n");

        let mut input = String::new();
        let _ = std::io::stdin().read_line(&mut input);

        println!("Capturing cookies...");
        let cookies = page.get_cookies().await?;

        let mut cookie_map = HashMap::new();
        for cookie in cookies {
            cookie_map.insert(cookie.name.clone(), cookie.value.clone());
        }

        let session = SessionData {
            token: None,
            cookies: cookie_map,
            captured_at: Some(Utc::now().timestamp()),
            data: HashMap::new(),
        };

        self.session_cache.set(&self.session_key(), &session)?;

        println!("Session saved successfully ({} cookies).", session.cookies.len());

        drop(browser);
        handler_task.abort();

        Ok(())
    }
}

fn default_download_root() -> Result<PathBuf> {
    let base = dirs::data_dir().context("Could not find data directory")?;
    let dir = base.join("keepbook").join("downloads").join("chase");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create download root: {}", dir.display()))?;
    Ok(dir)
}

async fn launch_browser(profile_dir: &Path) -> Result<(Browser, chromiumoxide::handler::Handler)> {
    let chrome_path = find_chrome().context(
        "Chrome/Chromium not found. Please install Chrome or Chromium to use Chase sync.",
    )?;

    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path)
        .with_head()
        .viewport(None)
        .user_data_dir(profile_dir)
        .arg("--disable-blink-features=AutomationControlled")
        .arg("--disable-infobars")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to configure browser: {e}"))?;

    let (browser, handler) = Browser::launch(config)
        .await
        .context("Failed to launch browser")?;

    Ok((browser, handler))
}

async fn setup_download_handling(page: &chromiumoxide::Page, download_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(download_dir)?;

    let download_params = SetDownloadBehaviorParams::builder()
        .behavior(SetDownloadBehaviorBehavior::Allow)
        .download_path(download_dir.display().to_string())
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build download params: {e}"))?;

    page.execute(download_params).await?;
    Ok(())
}

async fn apply_cookies(page: &chromiumoxide::Page, session: &SessionData) -> Result<()> {
    let mut cookies = Vec::new();
    for (name, value) in &session.cookies {
        let mut cookie = CookieParam::new(name.clone(), value.clone());
        cookie.url = Some(CHASE_LOGIN_URL.to_string());
        cookies.push(cookie);
    }

    if !cookies.is_empty() {
        page.set_cookies(cookies).await?;
    }

    Ok(())
}

async fn watch_for_downloads(
    download_dir: &Path,
    timeout: Duration,
    idle: Duration,
) -> Result<Vec<PathBuf>> {
    use std::collections::HashSet;

    let initial: HashSet<PathBuf> = std::fs::read_dir(download_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();

    let mut found: HashSet<PathBuf> = HashSet::new();
    let poll = Duration::from_millis(500);
    let start = std::time::Instant::now();
    let mut last_new = None::<std::time::Instant>;

    loop {
        tokio::time::sleep(poll).await;

        let current: Vec<PathBuf> = std::fs::read_dir(download_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();

        for file in current {
            if initial.contains(&file) || found.contains(&file) {
                continue;
            }

            let filename = file.file_name().unwrap_or_default().to_string_lossy();
            if filename.ends_with(".crdownload") {
                continue;
            }

            found.insert(file);
            last_new = Some(std::time::Instant::now());
        }

        if start.elapsed() > timeout {
            break;
        }

        if !found.is_empty() {
            if let Some(last) = last_new {
                if last.elapsed() >= idle {
                    break;
                }
            }
        }
    }

    Ok(found.into_iter().collect())
}

/// Find Chrome/Chromium executable.
fn find_chrome() -> Option<String> {
    if let Ok(output) = std::process::Command::new("which")
        .arg("google-chrome")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    if let Ok(output) = std::process::Command::new("which").arg("chromium").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    let candidates = [
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
        "/run/current-system/sw/bin/google-chrome",
        "/run/current-system/sw/bin/chromium",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ];

    for candidate in candidates {
        if std::path::Path::new(candidate).exists() {
            return Some(candidate.to_string());
        }
    }
    None
}

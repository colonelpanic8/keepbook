//! Chase Browser Automation - Proof of Concept
//!
//! This example tests whether we can:
//! 1. Launch a browser and navigate to Chase
//! 2. Allow user to complete login (with 2FA)
//! 3. Detect successful login
//! 4. Trigger a QFX download and capture the file
//!
//! Run with:
//!   cargo run --example chase_browser_poc

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::browser::{
    SetDownloadBehaviorBehavior, SetDownloadBehaviorParams,
};
use chromiumoxide::cdp::browser_protocol::fetch::{
    self, EventRequestPaused, RequestPattern, RequestStage,
};
use chromiumoxide::Page;
use futures::StreamExt;
use tokio::sync::Mutex;

const CHASE_LOGIN_URL: &str = "https://www.chase.com";
const DOWNLOAD_DIR: &str = "/tmp/chase_downloads";

/// Find Chrome/Chromium executable.
fn find_chrome() -> Option<String> {
    // First try using `which` to find chrome in PATH
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

    // Fall back to known paths
    let candidates = [
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
        // NixOS
        "/run/current-system/sw/bin/google-chrome",
        "/run/current-system/sw/bin/chromium",
        // macOS
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

/// Set up download behavior to save files to our directory
async fn setup_download_handling(page: &Page) -> Result<()> {
    // Create download directory
    std::fs::create_dir_all(DOWNLOAD_DIR)?;

    // Set download behavior - files will go to DOWNLOAD_DIR
    let download_params = SetDownloadBehaviorParams::builder()
        .behavior(SetDownloadBehaviorBehavior::Allow)
        .download_path(DOWNLOAD_DIR)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build download params: {e}"))?;
    page.execute(download_params).await?;

    println!("Download directory set to: {DOWNLOAD_DIR}");
    Ok(())
}

/// Watch for new files in download directory
async fn watch_for_downloads() -> Result<Option<PathBuf>> {
    let download_path = PathBuf::from(DOWNLOAD_DIR);

    // Get initial file list
    let initial_files: Vec<_> = std::fs::read_dir(&download_path)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();

    println!("Watching for new files in {DOWNLOAD_DIR}...");
    println!("Initial files: {}", initial_files.len());

    // Poll for new files (simple approach)
    let timeout = Duration::from_secs(60);
    let start = std::time::Instant::now();
    let poll_interval = Duration::from_millis(500);

    loop {
        tokio::time::sleep(poll_interval).await;

        let current_files: Vec<_> = std::fs::read_dir(&download_path)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();

        // Find new files (excluding .crdownload temp files)
        for file in &current_files {
            if !initial_files.contains(file) {
                let filename = file.file_name().unwrap_or_default().to_string_lossy();
                if !filename.ends_with(".crdownload") {
                    println!("New file detected: {}", file.display());
                    return Ok(Some(file.clone()));
                }
            }
        }

        if start.elapsed() > timeout {
            println!("Timeout waiting for download");
            return Ok(None);
        }
    }
}

/// Attempt to intercept QFX file content directly from network responses
async fn setup_response_interception(page: &Page) -> Result<Arc<Mutex<Option<String>>>> {
    let captured_qfx: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let qfx_clone = captured_qfx.clone();

    // Set up request interception for potential QFX downloads
    let patterns = vec![
        RequestPattern {
            url_pattern: Some("*.qfx*".to_string()),
            resource_type: None,
            request_stage: Some(RequestStage::Response),
        },
        RequestPattern {
            url_pattern: Some("*download*".to_string()),
            resource_type: None,
            request_stage: Some(RequestStage::Response),
        },
        RequestPattern {
            url_pattern: Some("*export*".to_string()),
            resource_type: None,
            request_stage: Some(RequestStage::Response),
        },
    ];

    page.execute(fetch::EnableParams {
        patterns: Some(patterns),
        handle_auth_requests: None,
    })
    .await?;

    let mut request_events = page.event_listener::<EventRequestPaused>().await?;
    let page_clone = page.clone();

    tokio::spawn(async move {
        while let Some(event) = request_events.next().await {
            let url = &event.request.url;
            println!("Intercepted request: {url}");

            // Check if this looks like a QFX download
            if url.contains(".qfx") || url.contains("download") || url.contains("export") {
                println!("Potential QFX download detected!");

                // Try to get response body
                if let Ok(response) = page_clone
                    .execute(fetch::GetResponseBodyParams::new(event.request_id.clone()))
                    .await
                {
                    let body = if response.base64_encoded {
                        // Decode base64
                        if let Ok(decoded) = base64::Engine::decode(
                            &base64::engine::general_purpose::STANDARD,
                            &response.body,
                        ) {
                            String::from_utf8_lossy(&decoded).to_string()
                        } else {
                            response.body.clone()
                        }
                    } else {
                        response.body.clone()
                    };

                    // Check if it looks like OFX/QFX content
                    if body.contains("OFXHEADER") || body.contains("<OFX>") {
                        println!("Captured QFX content! ({} bytes)", body.len());
                        let mut guard = qfx_clone.lock().await;
                        *guard = Some(body);
                    }
                }
            }

            // Continue the request
            let _ = page_clone
                .execute(fetch::ContinueRequestParams {
                    request_id: event.request_id.clone(),
                    url: None,
                    method: None,
                    post_data: None,
                    headers: None,
                    intercept_response: None,
                })
                .await;
        }
    });

    Ok(captured_qfx)
}

/// Main POC flow
async fn run_poc() -> Result<()> {
    println!("Chase Browser Automation POC");
    println!("============================\n");

    // Find Chrome
    let chrome_path =
        find_chrome().context("Chrome/Chromium not found. Please install Chrome or Chromium.")?;
    println!("Using browser: {chrome_path}\n");

    // Configure browser with anti-detection flags
    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path)
        .with_head() // Show the browser window
        .viewport(None)
        .arg("--disable-blink-features=AutomationControlled")
        .arg("--disable-infobars")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to configure browser: {e}"))?;

    let (browser, mut handler) = Browser::launch(config)
        .await
        .context("Failed to launch browser")?;

    // Spawn the handler task
    let handler_task = tokio::spawn(async move { while (handler.next().await).is_some() {} });

    // Create a new page
    let page = browser.new_page("about:blank").await?;

    // Set up download handling
    setup_download_handling(&page).await?;

    // Set up response interception (optional - may not work for all download types)
    let captured_qfx = setup_response_interception(&page).await?;

    // Navigate to Chase
    println!("Navigating to Chase...");
    page.goto(CHASE_LOGIN_URL).await?;

    println!("\n========================================");
    println!("INSTRUCTIONS:");
    println!("1. Log in to your Chase account");
    println!("2. Complete any 2FA verification");
    println!("3. Navigate to an account");
    println!("4. Click 'Download account activity'");
    println!("5. Select date range and QFX format");
    println!("6. Click Download");
    println!("========================================\n");

    // Start watching for downloads in background
    let download_task = tokio::spawn(async move { watch_for_downloads().await });

    // Wait for user to complete the flow or timeout
    let timeout = Duration::from_secs(600); // 10 minute timeout
    let start = std::time::Instant::now();

    println!("Waiting for download (timeout: 10 minutes)...\n");

    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Check if we captured QFX via interception
        {
            let guard = captured_qfx.lock().await;
            if let Some(content) = guard.as_ref() {
                println!("\n✓ QFX captured via network interception!");
                println!("Content preview:");
                println!("{}", &content[..500.min(content.len())]);
                break;
            }
        }

        // Check if download task completed
        if download_task.is_finished() {
            match download_task.await {
                Ok(Ok(Some(path))) => {
                    println!("\n✓ File downloaded: {}", path.display());

                    // Read and preview the file
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        println!("\nFile content preview:");
                        println!("{}", &content[..500.min(content.len())]);

                        if content.contains("OFXHEADER") || content.contains("<OFX>") {
                            println!("\n✓ Confirmed: This is a valid OFX/QFX file!");
                        }
                    }
                    break;
                }
                Ok(Ok(None)) => {
                    println!("Download watch timed out");
                    break;
                }
                Ok(Err(e)) => {
                    println!("Download watch error: {e}");
                    break;
                }
                Err(e) => {
                    println!("Task error: {e}");
                    break;
                }
            }
        }

        if start.elapsed() > timeout {
            println!("Overall timeout reached");
            break;
        }
    }

    // Get current URL for debugging
    if let Ok(url) = page.url().await {
        println!("\nFinal URL: {}", url.unwrap_or_default());
    }

    println!("\nPOC complete. Press Ctrl+C to close browser.");

    // Keep browser open for inspection
    tokio::signal::ctrl_c().await?;

    // Clean up
    drop(browser);
    handler_task.abort();

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    run_poc().await
}

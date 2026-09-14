//! Browser automation for Chase: login, cookie capture, and the browser-context
//! API fallback used when the direct HTTP client's session is rejected.

use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::cdp::browser_protocol::network::CookieParam;
use chrono::Utc;
use futures::StreamExt;

use crate::credentials::{SessionData, StoredCookie};
use crate::sync::chase::api::{
    ActivityAccount, AppDataResponse, CardDetailResponse, ChaseActivity, MortgageDetailResponse,
    TransactionsResponse,
};

pub(crate) struct BrowserApiClient {
    _browser: Browser,
    handler_task: tokio::task::JoinHandle<()>,
    page: chromiumoxide::Page,
}

impl Drop for BrowserApiClient {
    fn drop(&mut self) {
        self.handler_task.abort();
    }
}

impl BrowserApiClient {
    pub(crate) async fn connect(profile_dir: &Path, session: &SessionData) -> Result<Self> {
        let (browser, mut handler) = launch_browser(profile_dir, false).await?;
        let handler_task = tokio::spawn(async move { while (handler.next().await).is_some() {} });
        let page = browser.new_page("about:blank").await?;

        page.goto("https://secure.chase.com/web/auth/dashboard")
            .await
            .ok();
        apply_cookies(&page, session).await.ok();
        page.goto("https://secure.chase.com/web/auth/dashboard")
            .await
            .ok();
        ensure_logged_in_with_timeout(&page, Duration::from_secs(30)).await?;

        Ok(Self {
            _browser: browser,
            handler_task,
            page,
        })
    }

    pub(crate) async fn capture_session(&self) -> Result<SessionData> {
        session_from_page(&self.page).await
    }

    async fn post_json(&self, path: &str, body: &str) -> Result<serde_json::Value> {
        browser_fetch_json(&self.page, "POST", path, Some(body)).await
    }

    async fn get_json(&self, path: &str) -> Result<serde_json::Value> {
        browser_fetch_json(&self.page, "GET", path, None).await
    }

    pub(crate) async fn test_auth(&self) -> Result<()> {
        let value = self
            .post_json(
                "/svc/rl/accounts/secure/v1/dashboard/data/list",
                "context=GWM_OVD_NEW_PBM",
            )
            .await?;
        let resp: AppDataResponse =
            serde_json::from_value(value).context("Failed to parse Chase dashboard response")?;
        if resp.code != "SUCCESS" {
            anyhow::bail!(
                "Chase auth test failed in browser context: code={}",
                resp.code
            );
        }
        Ok(())
    }

    pub(crate) async fn get_accounts(&self) -> Result<Vec<ActivityAccount>> {
        let value = self
            .post_json(
                "/svc/rl/accounts/secure/v1/dashboard/data/list",
                "context=GWM_OVD_NEW_PBM",
            )
            .await?;
        let resp: AppDataResponse =
            serde_json::from_value(value).context("Failed to parse Chase dashboard response")?;
        for cached in &resp.cache {
            if cached.url.contains("activity/options/list") {
                if let Some(accounts) = cached.response.get("accounts") {
                    let accounts: Vec<ActivityAccount> =
                        serde_json::from_value(accounts.clone())
                            .context("Failed to parse accounts from dashboard cache")?;
                    return Ok(accounts);
                }
            }
        }
        anyhow::bail!("Could not find accounts in Chase dashboard response");
    }

    pub(crate) async fn get_card_detail(&self, account_id: i64) -> Result<CardDetailResponse> {
        let value = self
            .post_json(
                "/svc/rr/accounts/secure/v2/account/detail/card/list",
                &format!("accountId={account_id}"),
            )
            .await?;
        serde_json::from_value(value).context("Failed to parse Chase card detail")
    }

    pub(crate) async fn get_mortgage_detail(
        &self,
        account_id: i64,
    ) -> Result<MortgageDetailResponse> {
        let value = self
            .post_json(
                "/svc/rr/accounts/secure/v2/account/detail/mortgage/list",
                &format!("accountId={account_id}"),
            )
            .await?;
        serde_json::from_value(value).context("Failed to parse Chase mortgage detail")
    }

    pub(crate) async fn get_card_transactions(
        &self,
        account_id: i64,
        record_count: u32,
        pagination_key: Option<String>,
    ) -> Result<TransactionsResponse> {
        let mut path = format!(
            "/svc/rr/accounts/secure/gateway/credit-card/transactions/inquiry-maintenance/etu-transactions/v4/accounts/transactions?digital-account-identifier={account_id}&provide-available-statement-indicator=true&record-count={record_count}&sort-order-code=D&sort-key-code=T"
        );
        path.push_str(&crate::sync::chase::api::transaction_date_range_params());

        if let Some(key) = pagination_key {
            path.push_str(&format!("&next-page-key={key}"));
        }

        let value = self.get_json(&path).await?;
        serde_json::from_value(value).context("Failed to parse Chase transactions response")
    }

    pub(crate) async fn get_all_card_transactions(
        &self,
        account_id: i64,
    ) -> Result<Vec<ChaseActivity>> {
        // The direct HTTP client and browser-backed API client should behave the same
        // for transaction history: paginate until Chase stops, with safety guards.
        //
        // We reuse the shared pagination helper in the API module so fixes apply to both.
        let page_size = crate::sync::chase::api::DEFAULT_CARD_TXN_PAGE_SIZE;
        let max_transactions = crate::sync::chase::api::max_card_transactions();

        crate::sync::chase::api::get_all_card_transactions_paginated(
            "Chase(browser)",
            page_size,
            max_transactions,
            |key| self.get_card_transactions(account_id, page_size, key),
        )
        .await
    }
}

pub(crate) fn default_profile_root() -> Result<PathBuf> {
    let base = dirs::cache_dir().context("Could not find cache directory")?;
    let dir = base.join("keepbook").join("chase").join("profiles");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create profile root: {}", dir.display()))?;
    Ok(dir)
}

pub(crate) async fn ensure_logged_in_with_timeout(
    page: &chromiumoxide::Page,
    timeout: Duration,
) -> Result<()> {
    let check_js = r#"(function() {
      const url = String(window.location && window.location.href || '');
      const txt = (document.body && document.body.innerText || '').toLowerCase();
      const hasSignIn = txt.includes('sign in') || txt.includes('signin') || txt.includes('sign on') || txt.includes('enroll');
      const hasLogout = txt.includes('sign out') || txt.includes('log out') || txt.includes('logout');
      const isSecure = url.includes('secure.chase.com') || url.includes('/web/auth/');
      const loginIframe = document.querySelector('iframe[name=\"logonbox\"], iframe#logonbox');
      const hasLoginIframe = !!loginIframe;
      let iframeSnippet = '';
      if (loginIframe) {
        try {
          const doc = loginIframe.contentDocument || (loginIframe.contentWindow && loginIframe.contentWindow.document);
          if (doc && doc.body) {
            iframeSnippet = String(doc.body.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 160);
          }
        } catch (_) {}
      }
      const isLogonUrl = /\/logon\/|#\/logon\//i.test(url);
      return { url, hasSignIn, hasLogout, isSecure, hasLoginIframe, isLogonUrl, iframeSnippet };
    })()"#;

    let deadline = std::time::Instant::now() + timeout;
    loop {
        let v: serde_json::Value = match page.evaluate(check_js).await {
            Ok(value) => value.into_value()?,
            Err(err) if is_transient_execution_context_error(&err.to_string()) => {
                if std::time::Instant::now() > deadline {
                    return Err(err.into());
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            Err(err) => return Err(err.into()),
        };
        let url = v
            .get("url")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();
        let has_sign_in = v.get("hasSignIn").and_then(|x| x.as_bool()).unwrap_or(true);
        let has_logout = v
            .get("hasLogout")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        let is_secure = v.get("isSecure").and_then(|x| x.as_bool()).unwrap_or(false);
        let has_login_iframe = v
            .get("hasLoginIframe")
            .and_then(|x| x.as_bool())
            .unwrap_or(true);
        let is_logon_url = v
            .get("isLogonUrl")
            .and_then(|x| x.as_bool())
            .unwrap_or(false);
        let iframe_snippet = v
            .get("iframeSnippet")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string();

        if ((is_secure && !has_sign_in) || has_logout) && !has_login_iframe && !is_logon_url {
            return Ok(());
        }

        if std::time::Instant::now() > deadline {
            anyhow::bail!(
                "Chase session does not appear to be logged in (url={url}, has_sign_in={has_sign_in}, has_logout={has_logout}, has_login_iframe={has_login_iframe}, is_logon_url={is_logon_url}, iframe_snippet={iframe_snippet}). Run `keepbook auth chase login` again."
            );
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

pub(crate) async fn autofill_login_iframe(
    page: &chromiumoxide::Page,
    username: &str,
    password: &str,
) -> Result<()> {
    let creds = serde_json::json!({ "username": username, "password": password });

    let js: String = format!(
        r#"(function(creds) {{
  function fire(el, type) {{
    try {{ el.dispatchEvent(new Event(type, {{ bubbles: true }})); }} catch (_) {{}}
  }}
  function fireMouse(win, el, type) {{
    try {{
      el.dispatchEvent(new win.MouseEvent(type, {{ bubbles: true, cancelable: true, view: win }}));
      return true;
    }} catch (_) {{}}
    return false;
  }}
  const iframeNames = Array.from(document.querySelectorAll('iframe')).map(f => (f.getAttribute('name') || f.id || '').toString()).filter(Boolean).slice(0, 10);
  const iframe = document.querySelector('iframe[name="logonbox"], iframe#logonbox');
  if (!iframe) return {{ ok: false, error: "login iframe not found", iframeNames }};
  const doc = iframe.contentDocument || (iframe.contentWindow && iframe.contentWindow.document);
  if (!doc) return {{ ok: false, error: "iframe document not accessible", iframeNames }};
  const win = doc.defaultView || iframe.contentWindow || window;

  const user = doc.querySelector('#userId-input-field-input') || doc.querySelector('input[name="username"]');
  const pass = doc.querySelector('#password-input-field-input') || doc.querySelector('input[name="password"][type="password"]');
  const btn = doc.querySelector('#signin-button') || doc.querySelector('button[type="submit"]');
  if (!user || !pass || !btn) {{
    return {{
      ok: false,
      error: "missing login controls",
      iframeNames,
      have: {{
        user: !!user,
        pass: !!pass,
        btn: !!btn
      }}
    }};
  }}

  try {{ user.focus(); }} catch (_) {{}}
  user.value = String(creds.username || "");
  fire(user, "input"); fire(user, "change");

  try {{ pass.focus(); }} catch (_) {{}}
  pass.value = String(creds.password || "");
  fire(pass, "input"); fire(pass, "change");

  // Submit. Chase can be picky about event types/context; try multiple strategies.
  let submittedBy = null;
  const form = btn.form || pass.closest('form') || user.closest('form');
  if (form && typeof form.requestSubmit === 'function') {{
    try {{ form.requestSubmit(btn); submittedBy = 'requestSubmit'; }} catch (_) {{}}
  }}
  if (!submittedBy && form && typeof form.submit === 'function') {{
    try {{ form.submit(); submittedBy = 'form.submit'; }} catch (_) {{}}
  }}
  if (!submittedBy) {{
    try {{ btn.focus(); }} catch (_) {{}}
    try {{ btn.click(); submittedBy = 'btn.click'; }} catch (_) {{}}
  }}
  if (!submittedBy) {{
    // Dispatch in the iframe's window context.
    if (fireMouse(win, btn, 'mousedown') || fireMouse(win, btn, 'pointerdown')) {{}}
    if (fireMouse(win, btn, 'mouseup') || fireMouse(win, btn, 'pointerup')) {{}}
    if (fireMouse(win, btn, 'click')) submittedBy = 'dispatch(click)';
  }}
  if (!submittedBy) {{
    // Last resort: press Enter in password field.
    try {{
      pass.focus();
      pass.dispatchEvent(new win.KeyboardEvent('keydown', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true }}));
      pass.dispatchEvent(new win.KeyboardEvent('keyup', {{ key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true }}));
      submittedBy = 'enter';
    }} catch (_) {{}}
  }}

  return {{ ok: true, submittedBy, btnDisabled: !!btn.disabled }};
}})({creds})"#
    );

    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        let v: serde_json::Value = match page.evaluate(js.clone()).await {
            Ok(value) => value.into_value()?,
            Err(err) if is_transient_execution_context_error(&err.to_string()) => {
                if std::time::Instant::now() >= deadline {
                    return Err(err.into());
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
            Err(err) => return Err(err.into()),
        };
        let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
        if ok {
            if let Some(by) = v.get("submittedBy").and_then(|x| x.as_str()) {
                eprintln!("Chase: submitted login ({by})");
            }
            return Ok(());
        }

        let err = v
            .get("error")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown error");

        // Chase loads the login iframe lazily and sometimes after redirects.
        // Retry briefly before giving up.
        let retryable = matches!(
            err,
            "login iframe not found" | "iframe document not accessible" | "missing login controls"
        );
        if retryable && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(250)).await;
            continue;
        }

        let iframe_names = v
            .get("iframeNames")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .take(10)
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        anyhow::bail!("Chase autofill JS failed: {err} (iframes={iframe_names})");
    }
}

fn is_transient_execution_context_error(message: &str) -> bool {
    message.contains("Cannot find context with specified id")
        || message.contains("Execution context was destroyed")
        || message.contains("Inspected target navigated or closed")
}

pub(crate) async fn maybe_prompt_and_fill_sms_code(page: &chromiumoxide::Page) -> Result<()> {
    // If a verification-code input is visible, prompt the user for the code and fill it.
    // Set KEEPBOOK_CHASE_SMS_CODE=1 to enable this behavior.
    let present_js = r#"(function() {
  function norm(s){ return String(s||'').toLowerCase(); }
  function isVisible(el){
    if (!el || !el.getBoundingClientRect) return false;
    const r = el.getBoundingClientRect();
    return r.width > 20 && r.height > 10;
  }
  function labelFor(doc, el){
    const aria = el.getAttribute('aria-label') || '';
    const ph = el.getAttribute('placeholder') || '';
    if (aria || ph) return aria || ph;
    if (el.id) {
      const lab = doc.querySelector('label[for=\"' + el.id.replace(/\"/g,'') + '\"]');
      if (lab) return lab.textContent || '';
    }
    return (el.name || el.id || '');
  }
  function hasCodeInput(doc){
    const inputs = Array.from(doc.querySelectorAll('input')).filter(isVisible);
    for (const el of inputs) {
      const meta = norm(labelFor(doc, el) + ' ' + (el.id||'') + ' ' + (el.name||''));
      const isCode = meta.includes('code') || meta.includes('passcode') || meta.includes('otp') || meta.includes('verification');
      if (!isCode) continue;
      const t = (el.getAttribute('type') || '').toLowerCase();
      if (t === 'hidden') continue;
      return true;
    }
    return false;
  }

  if (hasCodeInput(document)) return { ok: true };
  const iframes = Array.from(document.querySelectorAll('iframe'));
  for (const fr of iframes) {
    let doc = null;
    try { doc = fr.contentDocument || (fr.contentWindow && fr.contentWindow.document); } catch (_) { doc = null; }
    if (!doc) continue;
    if (hasCodeInput(doc)) return { ok: true };
  }
  return { ok: false };
})()"#;

    // The code input often appears only after submit + redirects; poll briefly.
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    loop {
        let v: serde_json::Value = page.evaluate(present_js).await?.into_value()?;
        let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
        if ok {
            break;
        }
        if std::time::Instant::now() >= deadline {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    eprintln!("\n========================================");
    eprintln!("Chase: enter SMS verification code (or blank to skip):");
    eprintln!("========================================\n");
    let mut code = String::new();
    let _ = std::io::stdin().read_line(&mut code);
    let code = code.trim().to_string();
    if code.is_empty() {
        return Ok(());
    }

    fill_sms_code(page, &code).await?;
    Ok(())
}

async fn fill_sms_code(page: &chromiumoxide::Page, code: &str) -> Result<()> {
    let code = serde_json::json!({ "code": code });
    let js: String = format!(
        r#"(function(payload) {{
  function norm(s){{ return String(s||'').trim().toLowerCase(); }}
  function isVisible(el){{
    if (!el || !el.getBoundingClientRect) return false;
    const r = el.getBoundingClientRect();
    return r.width > 20 && r.height > 10;
  }}
  function fire(el, type) {{
    try {{ el.dispatchEvent(new Event(type, {{ bubbles: true }})); }} catch (_) {{}}
  }}
  function labelFor(doc, el){{
    const aria = el.getAttribute('aria-label') || '';
    const ph = el.getAttribute('placeholder') || '';
    if (aria || ph) return aria || ph;
    if (el.id) {{
      const lab = doc.querySelector('label[for=\"' + el.id.replace(/\"/g,'') + '\"]');
      if (lab) return lab.textContent || '';
    }}
    return (el.name || el.id || '');
  }}
  function score(doc, el) {{
    const meta = norm(labelFor(doc, el) + ' ' + (el.id||'') + ' ' + (el.name||''));
    let s = 0;
    if (meta.includes('otp')) s += 6;
    if (meta.includes('passcode')) s += 6;
    if (meta.includes('verification')) s += 5;
    if (meta.includes('code')) s += 3;
    const t = norm(el.getAttribute('type') || '');
    if (t === 'tel') s += 2;
    return s;
  }}
  function findBest(doc) {{
    const inputs = Array.from(doc.querySelectorAll('input')).filter(isVisible);
    let best = null;
    let bestScore = 0;
    for (const el of inputs) {{
      const meta = norm(labelFor(doc, el) + ' ' + (el.id||'') + ' ' + (el.name||''));
      const isCode = meta.includes('code') || meta.includes('passcode') || meta.includes('otp') || meta.includes('verification');
      if (!isCode) continue;
      const t = norm(el.getAttribute('type') || '');
      if (t === 'hidden') continue;
      const s = score(doc, el);
      if (s > bestScore) {{
        bestScore = s;
        best = el;
      }}
    }}
    return best;
  }}
  function clickSubmit(doc) {{
    const btns = Array.from(doc.querySelectorAll('button,input[type=\"submit\"],input[type=\"button\"],[role=\"button\"],[role=\"link\"]')).filter(isVisible);
    const want = ['verify','continue','next','submit','confirm','done'];
    for (const b of btns) {{
      const txt = norm(b.textContent || b.value || b.getAttribute('aria-label') || '');
      if (!txt) continue;
      if (want.some(w => txt.includes(w))) {{
        try {{ b.click(); return true; }} catch (_) {{}}
        try {{ b.dispatchEvent(new MouseEvent('click', {{bubbles:true, cancelable:true, view:window}})); return true; }} catch (_) {{}}
      }}
    }}
    return false;
  }}
  function tryFill(doc) {{
    const el = findBest(doc);
    if (!el) return false;
    try {{ el.focus(); }} catch (_) {{}}
    el.value = String(payload.code || '');
    fire(el,'input'); fire(el,'change');
    clickSubmit(doc);
    return true;
  }}

  if (tryFill(document)) return {{ ok: true }};
  const iframes = Array.from(document.querySelectorAll('iframe'));
  for (const fr of iframes) {{
    let doc = null;
    try {{ doc = fr.contentDocument || (fr.contentWindow && fr.contentWindow.document); }} catch (_) {{ doc = null; }}
    if (!doc) continue;
    if (tryFill(doc)) return {{ ok: true }};
  }}
  return {{ ok: false, error: 'no code input found' }};
}})({code})"#
    );

    let v: serde_json::Value = page.evaluate(js).await?.into_value()?;
    if v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false) {
        return Ok(());
    }
    anyhow::bail!(
        "Could not find a verification-code input (you may need to complete 2FA in the browser)."
    );
}

async fn session_from_page(page: &chromiumoxide::Page) -> Result<SessionData> {
    let cookies = page.get_cookies().await?;

    let mut cookie_map = HashMap::new();
    let mut cookie_jar = Vec::new();
    for cookie in cookies {
        cookie_map.insert(cookie.name.clone(), cookie.value.clone());
        cookie_jar.push(StoredCookie {
            name: cookie.name,
            value: cookie.value,
            domain: cookie.domain,
            path: cookie.path,
            secure: cookie.secure,
            http_only: cookie.http_only,
            same_site: cookie.same_site.map(|s| format!("{s:?}")),
        });
    }

    Ok(SessionData {
        token: None,
        cookies: cookie_map,
        cookie_jar,
        captured_at: Some(Utc::now().timestamp()),
        data: HashMap::new(),
    })
}

pub(crate) async fn wait_for_valid_api_session(page: &chromiumoxide::Page) -> Result<SessionData> {
    // Login redirects and anti-bot checks can complete after the dashboard first appears.
    // Ensure the captured cookie jar can authenticate an API call before persisting it.
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    loop {
        let session = session_from_page(page).await?;
        // Force a secure origin for browser-context API fetches.
        page.goto("https://secure.chase.com/web/auth/dashboard")
            .await
            .ok();
        match browser_test_auth(page).await {
            Ok(()) => return Ok(session),
            Err(err) => {
                if std::time::Instant::now() >= deadline {
                    anyhow::bail!(
                        "Chase login completed in browser, but API session is not ready: {err}"
                    );
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn apply_cookies(page: &chromiumoxide::Page, session: &SessionData) -> Result<()> {
    let mut cookies = Vec::new();

    if !session.cookie_jar.is_empty() {
        for c in &session.cookie_jar {
            let mut cookie = CookieParam::new(c.name.clone(), c.value.clone());
            cookie.domain = Some(c.domain.clone());
            cookie.path = Some(c.path.clone());
            cookie.secure = Some(c.secure);
            cookie.http_only = Some(c.http_only);
            cookies.push(cookie);
        }
    } else {
        for (name, value) in &session.cookies {
            let mut cookie = CookieParam::new(name.clone(), value.clone());
            cookie.url = Some("https://www.chase.com".to_string());
            cookies.push(cookie);
        }
    }

    if !cookies.is_empty() {
        page.set_cookies(cookies).await?;
    }

    Ok(())
}

async fn browser_fetch_json(
    page: &chromiumoxide::Page,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<serde_json::Value> {
    let req = serde_json::json!({
        "method": method,
        "path": path,
        "body": body.unwrap_or(""),
    });

    let js = format!(
        r#"(async function(req) {{
  try {{
    const reqId = (globalThis.crypto && typeof globalThis.crypto.randomUUID === 'function')
      ? globalThis.crypto.randomUUID()
      : String(Date.now()) + '-' + String(Math.random()).slice(2);
    const opts = {{
      method: req.method,
      credentials: 'include',
      headers: {{
        'accept': 'application/json, text/plain, */*',
        'x-jpmc-csrf-token': 'NONE',
        'x-jpmc-channel': 'id=C30',
        'x-jpmc-client-request-id': reqId,
        'x-requested-with': 'XMLHttpRequest',
        'referer': 'https://secure.chase.com/web/auth/dashboard',
        'origin': 'https://secure.chase.com'
      }}
    }};
    if (String(req.method || '').toUpperCase() === 'POST') {{
      opts.headers['content-type'] = 'application/x-www-form-urlencoded; charset=UTF-8';
      opts.body = String(req.body || '');
    }}

    const res = await fetch(req.path, opts);
    const text = await res.text();
    return {{ ok: res.ok, status: res.status, text }};
  }} catch (e) {{
    return {{ ok: false, status: 0, text: String((e && e.message) || e || 'fetch failed') }};
  }}
}})({req})"#
    );

    let v: serde_json::Value = page.evaluate(js).await?.into_value()?;
    let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
    let status = v.get("status").and_then(|x| x.as_i64()).unwrap_or(0);
    let text = v
        .get("text")
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string();

    if !ok {
        anyhow::bail!(
            "Chase browser API request failed (method={method}, path={path}, status={status}): {}",
            text.chars().take(500).collect::<String>()
        );
    }

    serde_json::from_str(&text).with_context(|| {
        format!(
            "Failed to parse Chase browser API JSON (path={path}): {}",
            text.chars().take(200).collect::<String>()
        )
    })
}

async fn browser_test_auth(page: &chromiumoxide::Page) -> Result<()> {
    let value = browser_fetch_json(
        page,
        "POST",
        "/svc/rl/accounts/secure/v1/dashboard/data/list",
        Some("context=GWM_OVD_NEW_PBM"),
    )
    .await?;
    let resp: AppDataResponse =
        serde_json::from_value(value).context("Failed to parse Chase browser auth response")?;
    if resp.code != "SUCCESS" {
        anyhow::bail!("Chase browser auth test failed: code={}", resp.code);
    }
    Ok(())
}

async fn launch_browser(
    profile_dir: &Path,
    show_browser: bool,
) -> Result<(Browser, chromiumoxide::handler::Handler)> {
    let chrome_path = find_chrome().context(
        "Chrome/Chromium not found. Please install Chrome or Chromium to use Chase sync.",
    )?;

    let mut builder = BrowserConfig::builder();
    builder = builder
        .chrome_executable(chrome_path)
        .viewport(None)
        .user_data_dir(profile_dir)
        .arg("--disable-blink-features=AutomationControlled")
        .arg("--disable-infobars")
        .arg("--no-first-run")
        .arg("--no-default-browser-check");
    if show_browser {
        builder = builder.with_head();
    }
    let config = builder
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to configure browser: {e}"))?;

    let (browser, handler) = Browser::launch(config)
        .await
        .context("Failed to launch browser")?;

    Ok((browser, handler))
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

fn cleanup_profile_lock_artifacts(profile_dir: &Path) {
    for name in ["SingletonLock", "SingletonSocket"] {
        let path = profile_dir.join(name);
        let _ = std::fs::remove_file(path);
    }
}

fn kill_profile_browser_processes(profile_dir: &Path) {
    let needle = format!("user-data-dir={}", profile_dir.display());
    let output = match std::process::Command::new("pgrep")
        .arg("-f")
        .arg(&needle)
        .output()
    {
        Ok(o) => o,
        Err(_) => return,
    };

    if !output.status.success() {
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let pid = line.trim();
        if pid.is_empty() {
            continue;
        }
        let _ = std::process::Command::new("kill").arg(pid).status();
    }

    std::thread::sleep(Duration::from_millis(250));
}

/// Launch a headed browser on the login profile and hand back the pieces the
/// caller has to keep alive: the browser, its event-pump task, and a blank page.
pub(crate) async fn open_login_browser(
    profile_dir: &Path,
) -> Result<(Browser, tokio::task::JoinHandle<()>, chromiumoxide::Page)> {
    cleanup_profile_lock_artifacts(profile_dir);
    kill_profile_browser_processes(profile_dir);
    let (browser, mut handler) = launch_browser(profile_dir, true).await?;
    let handler_task = tokio::spawn(async move { while (handler.next().await).is_some() {} });
    let page = browser.new_page("about:blank").await?;
    Ok((browser, handler_task, page))
}

#[cfg(test)]
#[path = "../../../tests/unit/sync/chase/browser_tests.rs"]
mod browser_tests;

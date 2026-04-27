#!/usr/bin/env node

import fs from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';

const LOGIN_URL = 'https://client.schwab.com/Login/SignOn/CustomerCenterLogin.aspx';
const API_BASE = 'https://ausgateway.schwab.com/api/is.ClientSummaryExpWeb/V1/api';

function envEnabled(key, defaultValue) {
  const value = process.env[key];
  if (value === undefined) return defaultValue;
  return !['0', 'false', 'no'].includes(value.toLowerCase());
}

const DEBUG = envEnabled('KEEPBOOK_SCHWAB_AUTH_DEBUG', false);

function log(message) {
  if (DEBUG) process.stderr.write(`[schwab-headless] ${message}\n`);
}

async function writeDebugArtifact(page, label) {
  const dir = process.env.KEEPBOOK_SCHWAB_DEBUG_DIR;
  if (!dir) return;
  await fs.mkdir(dir, { recursive: true }).catch(() => {});
  const safeLabel = label.replace(/[^A-Za-z0-9_.-]+/g, '-');
  const base = path.join(dir, `${Date.now()}-${safeLabel}`);
  await page.screenshot({ path: `${base}.png`, fullPage: true }).catch(() => {});
  const html = await page.content().catch(() => '');
  if (html) await fs.writeFile(`${base}.html`, html).catch(() => {});
}

function loadPlaywright() {
  const roots = [
    process.env.KEEPBOOK_PLAYWRIGHT_ROOT,
    process.env.KEEPBOOK_PLAYWRIGHT_ROOT && path.join(process.env.KEEPBOOK_PLAYWRIGHT_ROOT, 'ts'),
    process.cwd(),
    path.join(process.cwd(), 'ts'),
    path.dirname(process.argv[1] || process.cwd()),
  ].filter(Boolean);
  const errors = [];
  for (const root of roots) {
    try {
      const require = createRequire(path.join(root, 'package.json'));
      return require('playwright-core');
    } catch (error) {
      errors.push(`${root}: ${error.message}`);
    }
  }
  throw new Error(
    `Could not load playwright-core. Run \`cd ts && yarn install\` or set NODE_PATH. Tried: ${errors.join('; ')}`,
  );
}

async function browserExecutable() {
  if (process.env.KEEPBOOK_SCHWAB_BROWSER_EXECUTABLE) {
    return process.env.KEEPBOOK_SCHWAB_BROWSER_EXECUTABLE;
  }
  if (process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH) {
    return process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH;
  }

  const candidates = [
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/run/current-system/sw/bin/google-chrome',
    '/run/current-system/sw/bin/chromium',
    '/usr/bin/google-chrome',
    '/usr/bin/google-chrome-stable',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    '/snap/bin/chromium',
  ];
  for (const candidate of candidates) {
    try {
      await fs.stat(candidate);
      return candidate;
    } catch {
      // Keep searching.
    }
  }
  return undefined;
}

function smsCodeCommand(service) {
  const specific = `KEEPBOOK_${service.toUpperCase().replace(/-/g, '_')}_SMS_CODE_COMMAND`;
  return process.env[specific] || process.env.KEEPBOOK_SMS_CODE_COMMAND || null;
}

function extractSmsCode(text) {
  const match = String(text || '').match(/(?:^|\D)(\d{4,8})(?:\D|$)/);
  return match ? match[1] : null;
}

async function runShell(command, env) {
  const { spawn } = await import('node:child_process');
  return await new Promise((resolve) => {
    const child = spawn('sh', ['-lc', command], {
      env: { ...process.env, ...env },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk.toString();
    });
    child.on('error', () => resolve(null));
    child.on('close', (code) => {
      if (code !== 0) return resolve(null);
      resolve(extractSmsCode(stdout) || extractSmsCode(stderr));
    });
  });
}

async function visible(locator) {
  try {
    return (await locator.count()) > 0 && (await locator.first().isVisible({ timeout: 250 }));
  } catch {
    return false;
  }
}

async function findVisible(frame, selectors) {
  for (const selector of selectors) {
    const locator = frame.locator(selector);
    if (await visible(locator)) return locator.first();
  }
  return null;
}

async function fillLoginForm(page, username, password) {
  const passwordSelectors = [
    'input[type="password"]',
    'input[name="password"]',
    'input[name*="pass" i]',
    'input[id*="password" i]',
    'input[autocomplete="current-password"]',
  ];
  const usernameSelectors = [
    'input[name="LoginId"]',
    'input[autocomplete="username"]',
    'input[id*="login" i]',
    'input[name*="user" i]',
    'input[name*="email" i]',
    'input[id*="user" i]',
    'input[type="email"]',
    'input[type="text"]',
  ];
  const submitSelectors = [
    'button[type="submit"]',
    'input[type="submit"]',
    'button[id*="sign" i]',
    'button[id*="submit" i]',
    'button[id*="login" i]',
    'button[name*="login" i]',
    'button[name*="submit" i]',
    'button[aria-label*="sign in" i]',
    'button[aria-label*="log in" i]',
    'a[role="button"][id*="login" i]',
  ];

  const deadline = Date.now() + 25_000;
  let lastError = 'login form not found';
  while (Date.now() < deadline) {
    log(`looking for login form url=${page.url()}`);
    for (const frame of page.frames()) {
      const pass = await findVisible(frame, passwordSelectors);
      if (!pass) continue;
      const user = await findVisible(frame, usernameSelectors);
      if (!user) {
        lastError = `username input not found in ${frame.url()}`;
        continue;
      }
      await user.fill(username);
      await pass.fill(password);

      const submit = await findVisible(frame, submitSelectors);
      if (submit) {
        await submit.click();
      } else {
        await pass.press('Enter');
      }
      log(`submitted login form frame=${frame.url()}`);
      return;
    }
    await page.waitForTimeout(250);
  }
  await writeDebugArtifact(page, 'login-form-not-found');
  throw new Error(`Schwab headless autofill failed: ${lastError}`);
}

async function pageText(page) {
  const chunks = [];
  for (const frame of page.frames()) {
    try {
      chunks.push(await frame.locator('body').innerText({ timeout: 250 }));
    } catch {
      // Frame may be navigating.
    }
  }
  return chunks.join('\n').toLowerCase();
}

async function clickSmsMethod(page) {
  for (const frame of page.frames()) {
    const body = await frame.locator('body').innerText({ timeout: 250 }).catch(() => '');
    const lower = body.toLowerCase();
    if (!/text me|sms|select a method|confirm your identity|how should we get in touch/.test(lower)) {
      continue;
    }

    const candidates = [
      '#otp_sms',
      '[role="button"][aria-label*="Text me" i]',
      'text=/text me/i',
      'text=/use another method/i',
      'input[type="radio"][value="otpMethod"]',
    ];
    for (const selector of candidates) {
      const locator = frame.locator(selector);
      if (await visible(locator)) {
        await locator.first().click({ timeout: 500 }).catch(() => {});
        await page.waitForTimeout(500);
        return true;
      }
    }

    const next = frame.getByRole('button', { name: /next|continue|send/i });
    if (await visible(next)) {
      await next.first().click({ timeout: 500 }).catch(() => {});
      await page.waitForTimeout(500);
      return true;
    }
  }
  return false;
}

async function fillSmsCode(page, code) {
  const inputSelectors = [
    'input[autocomplete="one-time-code"]',
    'input[name*="otp" i]',
    'input[id*="otp" i]',
    'input[name*="code" i]',
    'input[id*="code" i]',
    'input[aria-label*="code" i]',
    'input[placeholder*="code" i]',
    'input[inputmode="numeric"]',
    'input[maxlength="4"]',
    'input[maxlength="5"]',
    'input[maxlength="6"]',
    'input[maxlength="7"]',
    'input[maxlength="8"]',
  ];

  for (const frame of page.frames()) {
    const input = await findVisible(frame, inputSelectors);
    if (!input) continue;
    await input.fill(code);
    const submit = frame.getByRole('button', { name: /verify|submit|continue|next|sign|send/i });
    if (await visible(submit)) {
      await submit.first().click({ timeout: 1_000 }).catch(() => {});
    } else {
      await input.press('Enter').catch(() => {});
    }
    return true;
  }
  return false;
}

async function drivePostLogin(page) {
  const text = await pageText(page);
  const url = page.url();
  if (/security code|enter security code|confirm your identity|let's be sure it's you/.test(text)) {
    return 'waiting-for-mfa';
  }
  if (
    /login|signon|password|user id|username|session has either timed out/.test(text) &&
    /client\.schwab\.com\/(areas\/access\/login|login)|signon|sws-gateway.*login/i.test(url)
  ) {
    return 'waiting-for-login';
  }

  for (const frame of page.frames()) {
    for (const selector of ['text=/accounts/i', 'text=/positions/i', 'text=/portfolio/i', 'text=/summary/i']) {
      const locator = frame.locator(selector);
      if (await visible(locator)) {
        await locator.first().click({ timeout: 500 }).catch(() => {});
        return 'clicked-account-nav';
      }
    }
  }

  if (!/client\.schwab\.com/i.test(url) || /sws-gateway/i.test(url)) {
    await page.goto('https://client.schwab.com/clientapps/accounts/summary/', {
      waitUntil: 'domcontentloaded',
      timeout: 15_000,
    }).catch(() => {});
    return 'navigate-summary';
  }

  for (const apiUrl of [
    `${API_BASE}/Account?includeCustomGroups=true`,
    `${API_BASE}/AggregatedPositions`,
  ]) {
    await page.evaluate(async (urlToFetch) => {
      await fetch(urlToFetch, {
        credentials: 'include',
        headers: {
          accept: 'application/json',
          'schwab-client-channel': 'IO',
          'schwab-client-correlid': crypto.randomUUID ? crypto.randomUUID() : String(Date.now()),
          'schwab-env': 'PROD',
          'schwab-resource-version': '1',
        },
      });
    }, apiUrl).catch(() => {});
  }
  return 'fetch-api';
}

async function installTokenCapture(page, onCapture) {
  page.on('request', (request) => {
    const headers = request.headers();
    const auth = headers.authorization || headers.Authorization;
    if (auth && /^Bearer\s+/i.test(auth)) {
      onCapture(auth.replace(/^Bearer\s+/i, ''), request.url());
    }
  });

  await page.addInitScript(() => {
    if (window.__keepbookSchwabCaptureInstalled) return;
    window.__keepbookSchwabCaptureInstalled = true;
    window.__keepbookSchwabAuthCaptures = window.__keepbookSchwabAuthCaptures || [];

    function saveAuth(value, url) {
      const text = String(value || '');
      const match = text.match(/Bearer\s+([A-Za-z0-9._~+/=-]+)/i);
      if (!match) return;
      window.__keepbookSchwabAuthCaptures.push({
        token: match[1],
        url: String(url || location.href || ''),
        at: Date.now(),
      });
    }

    function inspectHeaders(headers, url) {
      if (!headers) return;
      try {
        if (typeof Headers !== 'undefined' && headers instanceof Headers) {
          for (const [key, value] of headers.entries()) {
            if (String(key).toLowerCase() === 'authorization') saveAuth(value, url);
          }
          return;
        }
      } catch {}
      if (Array.isArray(headers)) {
        for (const pair of headers) {
          if (pair && String(pair[0]).toLowerCase() === 'authorization') saveAuth(pair[1], url);
        }
        return;
      }
      if (typeof headers === 'object') {
        for (const key of Object.keys(headers)) {
          if (String(key).toLowerCase() === 'authorization') saveAuth(headers[key], url);
        }
      }
    }

    try {
      const originalFetch = window.fetch;
      if (typeof originalFetch === 'function') {
        window.fetch = function(input, init) {
          try {
            const url = typeof input === 'string' ? input : (input && input.url);
            if (input && input.headers) inspectHeaders(input.headers, url);
            if (init && init.headers) inspectHeaders(init.headers, url);
          } catch {}
          return originalFetch.apply(this, arguments);
        };
      }
    } catch {}

    try {
      const originalOpen = XMLHttpRequest.prototype.open;
      const originalSetRequestHeader = XMLHttpRequest.prototype.setRequestHeader;
      XMLHttpRequest.prototype.open = function(method, url) {
        try { this.__keepbookSchwabUrl = String(url || ''); } catch {}
        return originalOpen.apply(this, arguments);
      };
      XMLHttpRequest.prototype.setRequestHeader = function(name, value) {
        try {
          if (String(name).toLowerCase() === 'authorization') {
            saveAuth(value, this.__keepbookSchwabUrl);
          }
        } catch {}
        return originalSetRequestHeader.apply(this, arguments);
      };
    } catch {}
  });
}

async function installStealth(context) {
  await context.addInitScript(() => {
    try {
      Object.defineProperty(navigator, 'webdriver', { get: () => undefined });
    } catch {}
    try {
      Object.defineProperty(navigator, 'languages', { get: () => ['en-US', 'en'] });
    } catch {}
    try {
      Object.defineProperty(navigator, 'plugins', {
        get: () => [
          { name: 'Chrome PDF Plugin' },
          { name: 'Chrome PDF Viewer' },
          { name: 'Native Client' },
        ],
      });
    } catch {}
    try {
      window.chrome = window.chrome || {};
      window.chrome.runtime = window.chrome.runtime || {};
    } catch {}
    try {
      const originalQuery = window.navigator.permissions?.query;
      if (originalQuery) {
        window.navigator.permissions.query = (parameters) =>
          parameters && parameters.name === 'notifications'
            ? Promise.resolve({ state: Notification.permission })
            : originalQuery(parameters);
      }
    } catch {}
  });
}

async function pollPageCapture(page) {
  for (const frame of page.frames()) {
    try {
      const capture = await frame.evaluate(() => {
        const captures = Array.isArray(window.__keepbookSchwabAuthCaptures)
          ? window.__keepbookSchwabAuthCaptures
          : [];
        return captures[captures.length - 1] || null;
      });
      if (capture?.token) return capture;
    } catch {
      // Cross-navigation races are expected.
    }
  }
  return null;
}

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) {
    chunks.push(Buffer.from(chunk));
  }
  return Buffer.concat(chunks).toString('utf8');
}

async function main() {
  const input = JSON.parse(await readStdin());
  const username = String(input.username || '');
  const password = String(input.password || '');
  if (!username || !password) {
    throw new Error('Headless Schwab auth requires username and password credentials.');
  }

  const { chromium } = loadPlaywright();
  const executablePath = await browserExecutable();
  const headless = envEnabled('KEEPBOOK_SCHWAB_HEADLESS', true);
  const timeoutMs = Number(input.timeoutMs || process.env.KEEPBOOK_SCHWAB_AUTH_TIMEOUT_MS || 300_000);
  const command = smsCodeCommand('schwab');

  log(`launching browser headless=${headless} executable=${executablePath || '<playwright-default>'}`);
  const browser = await chromium.launch({
    headless,
    executablePath,
    args: [
      '--disable-blink-features=AutomationControlled',
      '--disable-infobars',
      '--no-first-run',
      '--no-default-browser-check',
    ],
  });
  const context = await browser.newContext({
    viewport: null,
    userAgent:
      process.env.KEEPBOOK_SCHWAB_USER_AGENT ||
      'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36',
  });
  await installStealth(context);
  const page = await context.newPage();
  let captured = null;
  await installTokenCapture(page, (token, url) => {
    captured = { token, url };
  });

  try {
    log(`navigating to login url=${input.loginUrl || LOGIN_URL}`);
    await page.goto(input.loginUrl || LOGIN_URL, { waitUntil: 'domcontentloaded', timeout: 45_000 });
    log(`login page loaded url=${page.url()}`);
    await fillLoginForm(page, username, password);

    const deadline = Date.now() + timeoutMs;
    let lastCode = null;
    let lastAction = 'submitted-login';
    let lastLogAt = 0;
    while (Date.now() < deadline) {
      const pageCapture = await pollPageCapture(page);
      if (pageCapture?.token) {
        captured = pageCapture;
      }
      if (captured?.token) break;

      lastAction = await drivePostLogin(page);
      if (DEBUG && Date.now() - lastLogAt > 5_000) {
        lastLogAt = Date.now();
        log(`poll action=${lastAction} url=${page.url()}`);
      }
      await clickSmsMethod(page).catch(() => false);

      if (command) {
        const code = await runShell(command, { KEEPBOOK_SMS_CODE_SERVICE: 'schwab' });
        if (code && code !== lastCode) {
          lastCode = code;
          const filled = await fillSmsCode(page, code);
          if (filled) lastAction = 'filled-sms-code';
        }
      } else if (lastAction === 'waiting-for-mfa') {
        throw new Error(
          'Schwab headless auth reached MFA but no KEEPBOOK_SCHWAB_SMS_CODE_COMMAND or KEEPBOOK_SMS_CODE_COMMAND is configured.',
        );
      }

      await page.waitForTimeout(1000);
    }

    if (!captured?.token) {
      await writeDebugArtifact(page, 'token-timeout');
      throw new Error(`Timed out waiting for Schwab bearer token; last action: ${lastAction}`);
    }

    const cookies = await context.cookies();
    const simpleCookies = Object.fromEntries(cookies.map((cookie) => [cookie.name, cookie.value]));
    const cookieJar = cookies.map((cookie) => ({
      name: cookie.name,
      value: cookie.value,
      domain: cookie.domain,
      path: cookie.path,
      secure: Boolean(cookie.secure),
      http_only: Boolean(cookie.httpOnly),
      same_site: cookie.sameSite || null,
    }));

    const output = {
      token: captured.token,
      api_base: API_BASE,
      cookies: simpleCookies,
      cookie_jar: cookieJar,
    };
    process.stdout.write(`${JSON.stringify(output)}\n`);
  } finally {
    await browser.close().catch(() => {});
  }
}

main().catch((error) => {
  process.stderr.write(`${error.stack || error.message || String(error)}\n`);
  process.exit(1);
});

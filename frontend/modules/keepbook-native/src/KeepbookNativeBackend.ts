import AsyncStorage from '@react-native-async-storage/async-storage';
import { AsyncStorageStorage } from './AsyncStorageStorage';
import { AsyncStorageMarketDataStore } from './AsyncStorageMarketDataStore';
import { portfolioHistoryNative } from './portfolioHistoryNative';
import { spendingReport } from '@keepbook/app/spending';
import { decStrRounded, formatDateYMD } from '@keepbook/app/format';
import { valueInReportingCurrencyBestEffort } from '@keepbook/app/value';
import { MarketDataService } from '@keepbook/market-data/service';
import { getFileContent, setFileContents } from './FileContentStore';
import { BalanceSnapshot, type AssetBalanceJSON } from '@keepbook/models/balance';
import { Id } from '@keepbook/models/id';
import { Decimal } from '@keepbook/decimal';

type ConnectionSummary = {
  id: string;
  name: string;
  synchronizer: string;
  status: string;
  created_at: string;
  last_sync_at?: string | null;
  last_sync_status?: string | null;
};

type AccountSummary = {
  id: string;
  name: string;
  connection_id: string;
  created_at: string;
  active: boolean;
  current_balance?: AccountBalanceSnapshotSummary;
};

type AccountBalanceSummary = AssetBalanceJSON & {
  value_in_base: string | null;
  base_currency: string;
};

type AccountBalanceSnapshotSummary = {
  timestamp: string;
  balances: AccountBalanceSummary[];
  total_value_in_base?: string | null;
  base_currency?: string;
  currency_decimals?: number;
};

export interface KeepbookNativeModuleLike {
  PI: number;
  setValueAsync(value: string): Promise<void>;
  hello(): string;
  version(): string;
  demoDataDir(): string;
  gitRepoDir(): string;
  gitDataDir(): string;
  initDemo(dataDir: string): Promise<string>;
  listConnections(dataDir: string): Promise<string>;
  listAccounts(dataDir: string): Promise<string>;
  gitSync(
    repoDir: string,
    host: string,
    repo: string,
    sshUser: string,
    privateKeyPem: string,
    branch: string,
    authToken: string,
  ): Promise<string>;
  portfolioHistory(
    dataDir: string,
    start: string | null,
    end: string | null,
    granularity: string | null,
  ): Promise<string>;
  spending(
    dataDir: string,
    start: string | null,
    end: string | null,
    period: string | null,
    groupBy: string | null,
    direction: string | null,
  ): Promise<string>;
}

function nowRfc3339(): string {
  return new Date().toISOString();
}

function storageKey(dataDir: string, kind: 'connections' | 'accounts'): string {
  return `keepbook.app.${dataDir}.${kind}`;
}

function fileStorageKey(dataDir: string, relativePath: string): string {
  return `keepbook.file.${dataDir}.${relativePath}`;
}

function manifestStorageKey(dataDir: string): string {
  return `keepbook.manifest.${dataDir}`;
}

function gitDataDirKey(): string {
  return 'git';
}

function parseOwnerRepo(repo: string): { owner: string; name: string } | null {
  const parts = repo.trim().split('/');
  if (parts.length !== 2) return null;
  const [owner, name] = parts.map((p) => p.trim()).filter(Boolean);
  if (!owner || !name) return null;
  return { owner, name };
}

function parseTomlStringValue(toml: string, key: string): string | null {
  const re = new RegExp(`^\\s*${key}\\s*=\\s*(\"(?:\\\\.|[^\"\\\\])*\")\\s*(?:#.*)?$`, 'm');
  const m = toml.match(re);
  return m ? parseTomlBasicString(m[1]) : null;
}

function parseTomlNumberValue(toml: string, key: string): number | undefined {
  const re = new RegExp(`^\\s*${key}\\s*=\\s*([0-9]+)\\s*(?:#.*)?$`, 'm');
  const m = toml.match(re);
  if (!m) return undefined;
  const value = Number(m[1]);
  return Number.isFinite(value) ? value : undefined;
}

function parseTomlBasicString(value: string): string {
  try {
    return JSON.parse(value) as string;
  } catch {
    return value.slice(1, -1);
  }
}

function parseTomlStringArrayValue(toml: string, key: string): string[] {
  const lines = toml.split(/\r?\n/);
  let arrayLiteral: string | null = null;
  let collecting = false;
  let buffer = '';

  for (const line of lines) {
    if (!collecting) {
      const start = line.match(new RegExp(`^\\s*${key}\\s*=\\s*(\\[.*)$`));
      if (!start) continue;
      collecting = true;
      buffer = start[1];
    } else {
      buffer += `\n${line}`;
    }

    if (buffer.includes(']')) {
      arrayLiteral = buffer.slice(0, buffer.indexOf(']') + 1);
      break;
    }
  }

  if (arrayLiteral === null) return [];

  const values: string[] = [];
  const itemRe = /"(?:\\.|[^"\\])*"/g;
  for (const item of arrayLiteral.match(itemRe) ?? []) {
    values.push(parseTomlBasicString(item));
  }
  return values;
}

function parseTomlSection(toml: string, section: string): string {
  const lines = toml.split(/\r?\n/);
  const selected: string[] = [];
  let inSection = false;

  for (const line of lines) {
    const header = line.match(/^\s*\[([^\]]+)\]\s*(?:#.*)?$/);
    if (header) {
      inSection = header[1].trim() === section;
      continue;
    }
    if (inSection) selected.push(line);
  }

  return selected.join('\n');
}

type TransactionIgnoreRuleConfig = {
  account_id?: string;
  account_name?: string;
  connection_id?: string;
  connection_name?: string;
  synchronizer?: string;
  description?: string;
  status?: string;
  amount?: string;
};

function parseTomlTransactionIgnoreRules(toml: string): TransactionIgnoreRuleConfig[] {
  const rules: TransactionIgnoreRuleConfig[] = [];
  let current: TransactionIgnoreRuleConfig | null = null;

  for (const line of toml.split(/\r?\n/)) {
    const arrayHeader = line.match(/^\s*\[\[([^\]]+)\]\]\s*(?:#.*)?$/);
    if (arrayHeader) {
      if (current !== null) rules.push(current);
      current = arrayHeader[1].trim() === 'ignore.transaction_rules' ? {} : null;
      continue;
    }

    const tableHeader = line.match(/^\s*\[([^\]]+)\]\s*(?:#.*)?$/);
    if (tableHeader) {
      if (current !== null) rules.push(current);
      current = null;
      continue;
    }

    if (current === null) continue;

    const field = line.match(
      /^\s*(account_id|account_name|connection_id|connection_name|synchronizer|description|status|amount)\s*=\s*("(?:\\.|[^"\\])*")\s*(?:#.*)?$/,
    );
    if (!field) continue;
    current[field[1] as keyof TransactionIgnoreRuleConfig] = parseTomlBasicString(field[2]);
  }

  if (current !== null) rules.push(current);
  return rules.filter((rule) => Object.values(rule).some((value) => value !== undefined));
}

async function fetchJson(url: string, authToken?: string): Promise<any> {
  const headers: Record<string, string> = {
    Accept: 'application/vnd.github+json',
  };
  if (authToken && authToken.trim()) {
    headers.Authorization = `Bearer ${authToken.trim()}`;
  }

  const res = await fetch(url, {
    method: 'GET',
    headers,
  });
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(`HTTP ${res.status} for ${url}${body ? `: ${body}` : ''}`);
  }
  return await res.json();
}

async function fetchText(url: string, authToken?: string): Promise<string> {
  const headers: Record<string, string> = {};
  if (authToken && authToken.trim()) {
    headers.Authorization = `Bearer ${authToken.trim()}`;
  }

  const res = await fetch(url, { method: 'GET', headers });
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(`HTTP ${res.status} for ${url}${body ? `: ${body}` : ''}`);
  }
  return await res.text();
}

async function fetchGitHubText(opts: {
  owner: string;
  name: string;
  branch: string;
  path: string;
  authToken: string;
}): Promise<string> {
  const { owner, name, branch, path, authToken } = opts;
  const url =
    `https://api.github.com/repos/${encodeURIComponent(owner)}/${encodeURIComponent(
      name,
    )}/contents/${path.split('/').map(encodeURIComponent).join('/')}?ref=${encodeURIComponent(branch)}`;
  const res = await fetch(url, {
    method: 'GET',
    headers: {
      Accept: 'application/vnd.github.raw',
      Authorization: `Bearer ${authToken.trim()}`,
    },
  });
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(`HTTP ${res.status} for ${url}${body ? `: ${body}` : ''}`);
  }
  return await res.text();
}

/** Like fetchGitHubText but returns null on 404 instead of throwing. */
async function fetchGitHubTextOptional(opts: {
  owner: string;
  name: string;
  branch: string;
  path: string;
  authToken: string;
}): Promise<string | null> {
  const { owner, name, branch, path, authToken } = opts;
  const url =
    `https://api.github.com/repos/${encodeURIComponent(owner)}/${encodeURIComponent(
      name,
    )}/contents/${path.split('/').map(encodeURIComponent).join('/')}?ref=${encodeURIComponent(branch)}`;
  const res = await fetch(url, {
    method: 'GET',
    headers: {
      Accept: 'application/vnd.github.raw',
      Authorization: `Bearer ${authToken.trim()}`,
    },
  });
  if (res.status === 404) return null;
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(`HTTP ${res.status} for ${url}${body ? `: ${body}` : ''}`);
  }
  return await res.text();
}

/** Fetch a file from raw.githubusercontent.com, returning null on 404. */
async function fetchTextOptional(url: string, authToken?: string): Promise<string | null> {
  const headers: Record<string, string> = {};
  if (authToken && authToken.trim()) {
    headers.Authorization = `Bearer ${authToken.trim()}`;
  }
  const res = await fetch(url, { method: 'GET', headers });
  if (res.status === 404) return null;
  if (!res.ok) {
    const body = await res.text().catch(() => '');
    throw new Error(`HTTP ${res.status} for ${url}${body ? `: ${body}` : ''}`);
  }
  return await res.text();
}

/**
 * Fetch a file from the repo, returning its content or null on 404.
 * Uses the GitHub Contents API when a token is available, otherwise
 * falls back to raw.githubusercontent.com.
 */
async function fetchRepoFile(opts: {
  owner: string;
  name: string;
  branch: string;
  path: string;
  token: string;
  rawBase: string;
}): Promise<string | null> {
  const { owner, name, branch, path, token, rawBase } = opts;
  if (token) {
    return fetchGitHubTextOptional({ owner, name, branch, path, authToken: token });
  }
  return fetchTextOptional(`${rawBase}/${path}`, undefined);
}

const MS_PER_HOUR = 60 * 60 * 1000;
const MS_PER_DAY = 24 * MS_PER_HOUR;
const KEEPBOOK_DATA_DEFAULT_BRANCH = 'master';

/**
 * Build a minimal ResolvedConfig compatible with the TS library.
 * The shape matches `ResolvedConfig` from `ts/src/config.ts` without
 * importing the module (which would pull in `node:path`).
 */
async function buildConfig(dataDir: string) {
  const configToml = await getFileContent(fileStorageKey(dataDir, 'keepbook.toml'));
  const displayToml = configToml ? parseTomlSection(configToml, 'display') : '';
  const spendingToml = configToml ? parseTomlSection(configToml, 'spending') : '';
  const configuredCurrencyDecimals = parseTomlNumberValue(displayToml, 'currency_decimals');

  return {
    data_dir: dataDir,
    reporting_currency: configToml
      ? (parseTomlStringValue(configToml, 'reporting_currency') ?? 'USD')
      : 'USD',
    display: {
      currency_decimals:
        configuredCurrencyDecimals !== undefined && Number.isInteger(configuredCurrencyDecimals)
          ? configuredCurrencyDecimals
          : 2,
    },
    refresh: {
      balance_staleness: 14 * MS_PER_DAY,
      price_staleness: 24 * MS_PER_HOUR,
    },
    tray: {
      history_points: 8,
      spending_windows_days: [7, 30, 90, 365],
    },
    spending: {
      ignore_accounts: parseTomlStringArrayValue(spendingToml, 'ignore_accounts'),
      ignore_connections: parseTomlStringArrayValue(spendingToml, 'ignore_connections'),
      ignore_tags: parseTomlStringArrayValue(spendingToml, 'ignore_tags'),
    },
    ignore: {
      transaction_rules: configToml ? parseTomlTransactionIgnoreRules(configToml) : [],
    },
    git: {
      auto_commit: false,
      auto_push: false,
      merge_master_before_command: false,
    },
  };
}

const KeepbookNative: KeepbookNativeModuleLike = {
  PI: Math.PI,

  async setValueAsync(_value: string): Promise<void> {},

  hello(): string {
    return 'Hello from keepbook TypeScript runtime';
  },

  version(): string {
    return 'ts';
  },

  demoDataDir(): string {
    return 'demo';
  },

  gitRepoDir(): string {
    // No local git checkout in the TS runtime.
    return '';
  },

  gitDataDir(): string {
    return gitDataDirKey();
  },

  async initDemo(_dataDir: string): Promise<string> {
    const dataDir = (_dataDir || 'demo').trim() || 'demo';
    try {
      const createdAt = nowRfc3339();

      const connections: ConnectionSummary[] = [
        {
          id: 'conn-demo',
          name: 'Demo Bank (ts)',
          synchronizer: 'demo',
          status: 'active',
          created_at: createdAt,
          last_sync_at: null,
          last_sync_status: null,
        },
      ];

      const accounts: AccountSummary[] = [
        {
          id: 'acct-demo',
          name: 'Demo Checking (ts)',
          connection_id: 'conn-demo',
          created_at: createdAt,
          active: true,
        },
      ];

      await AsyncStorage.multiSet([
        [storageKey(dataDir, 'connections'), JSON.stringify(connections)],
        [storageKey(dataDir, 'accounts'), JSON.stringify(accounts)],
      ]);
      return '';
    } catch (e) {
      return String(e);
    }
  },

  async listConnections(_dataDir: string): Promise<string> {
    const dataDir = (_dataDir || 'demo').trim() || 'demo';
    try {
      const v = await AsyncStorage.getItem(storageKey(dataDir, 'connections'));
      return v || '[]';
    } catch (e) {
      return JSON.stringify({ error: String(e) });
    }
  },

  async listAccounts(_dataDir: string): Promise<string> {
    const dataDir = (_dataDir || 'demo').trim() || 'demo';
    try {
      const v = await AsyncStorage.getItem(storageKey(dataDir, 'accounts'));
      const accounts = JSON.parse(v || '[]') as AccountSummary[];
      const storage = new AsyncStorageStorage(dataDir);
      const config = await buildConfig(dataDir);
      const baseCurrency = config.reporting_currency.trim().toUpperCase();
      const marketData = new MarketDataService(new AsyncStorageMarketDataStore(dataDir));
      const withBalances = await Promise.all(
        accounts.map(async (account) => {
          const latest = await storage.getLatestBalanceSnapshot(Id.fromString(account.id));
          if (latest === null) return account;
          const latestJson = BalanceSnapshot.toJSON(latest);
          const asOfDate = formatDateYMD(latest.timestamp);
          const assetCount = new Set(
            latestJson.balances.map((balance) => JSON.stringify(balance.asset)),
          ).size;
          const balances = await Promise.all(
            latest.balances.map(async (balance, index) => {
              const valueInBase = await valueInReportingCurrencyBestEffort(
                marketData,
                balance.asset,
                balance.amount,
                baseCurrency,
                asOfDate,
                config.display.currency_decimals,
              );
              return {
                ...latestJson.balances[index],
                value_in_base: valueInBase,
                base_currency: baseCurrency,
              };
            }),
          );
          const multiAssetTotal =
            assetCount > 1
              ? balances.every((balance) => balance.value_in_base !== null)
                ? decStrRounded(
                    balances.reduce(
                      (sum, balance) => sum.plus(new Decimal(balance.value_in_base ?? 0)),
                      new Decimal(0),
                    ),
                    config.display.currency_decimals,
                  )
                : null
              : undefined;
          return {
            ...account,
            current_balance: {
              timestamp: latestJson.timestamp,
              balances,
              currency_decimals: config.display.currency_decimals,
              ...(multiAssetTotal !== undefined
                ? { total_value_in_base: multiAssetTotal, base_currency: baseCurrency }
                : {}),
            },
          };
        }),
      );
      return JSON.stringify(withBalances);
    } catch (e) {
      return JSON.stringify({ error: String(e) });
    }
  },

  async gitSync(
    _repoDir: string,
    host: string,
    repo: string,
    _sshUser: string,
    _privateKeyPem: string,
    branch: string,
    authToken: string,
  ): Promise<string> {
    const trimmedHost = host.trim();
    const parsed = parseOwnerRepo(repo);
    const trimmedBranch = (branch || '').trim() || KEEPBOOK_DATA_DEFAULT_BRANCH;

    if (!parsed) {
      return 'repo must be in the form owner/name';
    }

    if (trimmedHost !== 'github.com' && trimmedHost !== 'www.github.com') {
      return 'TS sync currently supports only github.com';
    }

    const { owner, name } = parsed;
    const token = (authToken || '').trim();
    const dataDir = gitDataDirKey();

    try {
      let effectiveBranch = trimmedBranch;
      const treeUrlForBranch = (branchName: string) =>
        `https://api.github.com/repos/${encodeURIComponent(owner)}/${encodeURIComponent(
          name,
        )}/git/trees/${encodeURIComponent(branchName)}?recursive=1`;

      let tree;
      try {
        tree = await fetchJson(treeUrlForBranch(effectiveBranch), token);
      } catch (e) {
        const message = String(e);
        if (
          token &&
          message.includes('HTTP 404') &&
          effectiveBranch !== KEEPBOOK_DATA_DEFAULT_BRANCH
        ) {
          effectiveBranch = KEEPBOOK_DATA_DEFAULT_BRANCH;
          tree = await fetchJson(treeUrlForBranch(effectiveBranch), token);
        } else {
          throw e;
        }
      }

      const entries: Array<{ path?: string; type?: string }> = Array.isArray(tree?.tree)
        ? tree.tree
        : [];
      const blobPaths = entries.filter((e) => e.type === 'blob').map((e) => e.path || '');
      const configPaths = blobPaths.filter((p) => p === 'keepbook.toml');

      const connJsonPaths = blobPaths.filter((p) =>
        /^connections\/[^/]+\/connection\.json$/.test(p),
      );
      const connTomlPaths = new Set(
        blobPaths.filter((p) => /^connections\/[^/]+\/connection\.toml$/.test(p)),
      );
      const acctJsonPaths = blobPaths.filter((p) =>
        /^accounts\/[^/]+\/account\.json$/.test(p),
      );
      const accountConfigPaths = blobPaths.filter((p) =>
        /^accounts\/[^/]+\/account_config\.toml$/.test(p),
      );

      // Additional data file patterns
      const balancePaths = blobPaths.filter((p) =>
        /^accounts\/[^/]+\/balances\.jsonl$/.test(p),
      );
      const transactionPaths = blobPaths.filter((p) =>
        /^accounts\/[^/]+\/transactions\.jsonl$/.test(p),
      );
      const annotationPaths = blobPaths.filter((p) =>
        /^accounts\/[^/]+\/transaction_annotations\.jsonl$/.test(p),
      );
      const pricePaths = blobPaths.filter((p) => /^prices\/.+\/\d{4}\.jsonl$/.test(p));
      const fxPaths = blobPaths.filter((p) => /^fx\/[^/]+\/\d{4}\.jsonl$/.test(p));

      if (acctJsonPaths.length === 0) {
        return `Sync found 0 account files on ${owner}/${name}@${effectiveBranch}. Check that the repo layout contains accounts/{id}/account.json.`;
      }

      const rawBase = `https://raw.githubusercontent.com/${encodeURIComponent(owner)}/${encodeURIComponent(
        name,
      )}/${encodeURIComponent(effectiveBranch)}`;

      // Cache raw file content fetched during the metadata pass so we
      // can store it later without re-fetching.
      const rawContentCache = new Map<string, string>();

      const connections: ConnectionSummary[] = [];
      for (const p of connJsonPaths) {
        const id = p.split('/')[1] || '';
        const stateText = token
          ? await fetchGitHubText({ owner, name, branch: effectiveBranch, path: p, authToken: token })
          : await fetchText(`${rawBase}/${p}`);
        rawContentCache.set(p, stateText);
        const state = JSON.parse(stateText);

        let cfgName = id;
        let cfgSync = 'unknown';
        const tomlPath = `connections/${id}/connection.toml`;
        if (connTomlPaths.has(tomlPath)) {
          const tomlText = token
            ? await fetchGitHubText({
                owner,
                name,
                branch: effectiveBranch,
                path: tomlPath,
                authToken: token,
              })
            : await fetchText(`${rawBase}/${tomlPath}`);
          rawContentCache.set(tomlPath, tomlText);
          cfgName = parseTomlStringValue(tomlText, 'name') || cfgName;
          cfgSync = parseTomlStringValue(tomlText, 'synchronizer') || cfgSync;
        }

        connections.push({
          id: String(state?.id ?? id),
          name: cfgName,
          synchronizer: cfgSync,
          status: String(state?.status ?? 'unknown'),
          created_at: String(state?.created_at ?? nowRfc3339()),
          last_sync_at: state?.last_sync?.at ? String(state.last_sync.at) : null,
          last_sync_status: state?.last_sync?.status ? String(state.last_sync.status) : null,
        });
      }

      const accounts: AccountSummary[] = [];
      for (const p of acctJsonPaths) {
        const acctText = token
          ? await fetchGitHubText({ owner, name, branch: effectiveBranch, path: p, authToken: token })
          : await fetchText(`${rawBase}/${p}`);
        rawContentCache.set(p, acctText);
        const a = JSON.parse(acctText);
        accounts.push({
          id: String(a?.id ?? ''),
          name: String(a?.name ?? ''),
          connection_id: String(a?.connection_id ?? ''),
          created_at: String(a?.created_at ?? nowRfc3339()),
          active: Boolean(a?.active ?? true),
        });
      }

      // --- Fetch all additional data files ---

      // Collect all paths we need to fetch (balances, transactions,
      // annotations, prices, FX rates).
      const dataFilePaths = [
        ...configPaths,
        ...accountConfigPaths,
        ...balancePaths,
        ...transactionPaths,
        ...annotationPaths,
        ...pricePaths,
        ...fxPaths,
      ];

      // Fetch data files in parallel batches to avoid overwhelming the
      // network while still being much faster than sequential fetching.
      const BATCH_SIZE = 20;
      const fileEntries: Array<[string, string]> = [];
      const manifestPaths: string[] = [];

      for (let i = 0; i < dataFilePaths.length; i += BATCH_SIZE) {
        const batch = dataFilePaths.slice(i, i + BATCH_SIZE);
        const results = await Promise.all(
          batch.map(async (p) => {
            const content = await fetchRepoFile({
              owner,
              name,
              branch: effectiveBranch,
              path: p,
              token,
              rawBase,
            });
            return { path: p, content };
          }),
        );
        for (const { path: p, content } of results) {
          if (content != null) {
            fileEntries.push([fileStorageKey(dataDir, p), content]);
            manifestPaths.push(p);
          }
        }
      }

      // Add connection and account files from the cache (already
      // fetched during the metadata pass above).
      const metadataFilePaths = [
        ...connJsonPaths,
        ...Array.from(connTomlPaths),
        ...acctJsonPaths,
      ];
      for (const p of metadataFilePaths) {
        const content = rawContentCache.get(p);
        if (content != null) {
          fileEntries.push([fileStorageKey(dataDir, p), content]);
          manifestPaths.push(p);
        }
      }

      // Write everything to AsyncStorage in batches.
      const metadataEntries: Array<[string, string]> = [
        [storageKey(dataDir, 'connections'), JSON.stringify(connections)],
        [storageKey(dataDir, 'accounts'), JSON.stringify(accounts)],
        [manifestStorageKey(dataDir), JSON.stringify(manifestPaths)],
      ];

      // Publish file contents before the manifest so readers never see a
      // manifest for a sync whose config/transaction files are still missing.
      for (let i = 0; i < fileEntries.length; i += BATCH_SIZE) {
        await setFileContents(fileEntries.slice(i, i + BATCH_SIZE));
      }
      await AsyncStorage.multiSet(metadataEntries);

      return JSON.stringify({
        ok: true,
        branch: effectiveBranch,
        counts: {
          connections: connections.length,
          accounts: accounts.length,
          config_files: configPaths.length,
          account_config_files: accountConfigPaths.length,
          balance_files: balancePaths.length,
          transaction_files: transactionPaths.length,
          annotation_files: annotationPaths.length,
          price_files: pricePaths.length,
          fx_files: fxPaths.length,
          stored_files: manifestPaths.length,
        },
      });
    } catch (e) {
      const message = String(e);
      if (message.includes('HTTP 404')) {
        if (!token) {
          return `Repo ${owner}/${name} or branch ${trimmedBranch} was not found. If this is a private repo, enter a GitHub token with repo read access.`;
        }
        return `Repo ${owner}/${name} or branch ${trimmedBranch} was not found. Check the repo name and branch; this data repo currently uses master.`;
      }
      return String(e);
    }
  },

  async portfolioHistory(
    dataDir: string,
    start: string | null,
    end: string | null,
    granularity: string | null,
  ): Promise<string> {
    const effectiveDataDir = (dataDir || 'git').trim() || 'git';
    const effectiveGranularity = granularity ?? 'daily';
    try {
      const storage = new AsyncStorageStorage(effectiveDataDir);
      const marketDataStore = new AsyncStorageMarketDataStore(effectiveDataDir);
      const config = await buildConfig(effectiveDataDir);

      const result = await portfolioHistoryNative(
        storage,
        marketDataStore,
        config as any,
        {
          start: start ?? undefined,
          end: end ?? undefined,
          granularity: effectiveGranularity,
          includePrices: effectiveGranularity !== 'none',
        },
      );

      return JSON.stringify(result);
    } catch (e) {
      return JSON.stringify({ error: String(e) });
    }
  },

  async spending(
    dataDir: string,
    start: string | null,
    end: string | null,
    period: string | null,
    groupBy: string | null,
    direction: string | null,
  ): Promise<string> {
    const effectiveDataDir = (dataDir || 'git').trim() || 'git';
    try {
      const storage = new AsyncStorageStorage(effectiveDataDir);
      const marketDataStore = new AsyncStorageMarketDataStore(effectiveDataDir);
      const config = await buildConfig(effectiveDataDir);

      const result = await spendingReport(
        storage,
        marketDataStore,
        config as any,
        {
          start: start ?? undefined,
          end: end ?? undefined,
          period: period ?? 'monthly',
          direction: direction ?? 'outflow',
          group_by: groupBy ?? 'none',
        },
      );

      return JSON.stringify(result);
    } catch (e) {
      return JSON.stringify({ error: String(e) });
    }
  },
};

export default KeepbookNative;

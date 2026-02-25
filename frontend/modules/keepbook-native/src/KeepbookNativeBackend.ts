import AsyncStorage from '@react-native-async-storage/async-storage';

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
  const re = new RegExp(`^\\s*${key}\\s*=\\s*\"([^\"]*)\"\\s*$`, 'm');
  const m = toml.match(re);
  return m ? m[1] : null;
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
      return v || '[]';
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
    const trimmedBranch = (branch || '').trim() || 'main';

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
      const treeUrl = `https://api.github.com/repos/${encodeURIComponent(owner)}/${encodeURIComponent(
        name,
      )}/git/trees/${encodeURIComponent(trimmedBranch)}?recursive=1`;
      const tree = await fetchJson(treeUrl, token);

      const entries: Array<{ path?: string; type?: string }> = Array.isArray(tree?.tree)
        ? tree.tree
        : [];
      const blobPaths = entries.filter((e) => e.type === 'blob').map((e) => e.path || '');

      const connJsonPaths = blobPaths.filter((p) =>
        /^data\/connections\/[^/]+\/connection\.json$/.test(p),
      );
      const connTomlPaths = new Set(
        blobPaths.filter((p) => /^data\/connections\/[^/]+\/connection\.toml$/.test(p)),
      );
      const acctJsonPaths = blobPaths.filter((p) =>
        /^data\/accounts\/[^/]+\/account\.json$/.test(p),
      );

      // Additional data file patterns
      const balancePaths = blobPaths.filter((p) =>
        /^data\/accounts\/[^/]+\/balances\.jsonl$/.test(p),
      );
      const transactionPaths = blobPaths.filter((p) =>
        /^data\/accounts\/[^/]+\/transactions\.jsonl$/.test(p),
      );
      const annotationPaths = blobPaths.filter((p) =>
        /^data\/accounts\/[^/]+\/transaction_annotations\.jsonl$/.test(p),
      );
      const pricePaths = blobPaths.filter((p) => /^data\/prices\/.+\/\d{4}\.jsonl$/.test(p));
      const fxPaths = blobPaths.filter((p) => /^data\/fx\/[^/]+\/\d{4}\.jsonl$/.test(p));

      const rawBase = `https://raw.githubusercontent.com/${encodeURIComponent(owner)}/${encodeURIComponent(
        name,
      )}/${encodeURIComponent(trimmedBranch)}`;

      // Cache raw file content fetched during the metadata pass so we
      // can store it later without re-fetching.
      const rawContentCache = new Map<string, string>();

      const connections: ConnectionSummary[] = [];
      for (const p of connJsonPaths) {
        const id = p.split('/')[2] || '';
        const stateText = token
          ? await fetchGitHubText({ owner, name, branch: trimmedBranch, path: p, authToken: token })
          : await fetchText(`${rawBase}/${p}`);
        rawContentCache.set(p, stateText);
        const state = JSON.parse(stateText);

        let cfgName = id;
        let cfgSync = 'unknown';
        const tomlPath = `data/connections/${id}/connection.toml`;
        if (connTomlPaths.has(tomlPath)) {
          const tomlText = token
            ? await fetchGitHubText({
                owner,
                name,
                branch: trimmedBranch,
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
          ? await fetchGitHubText({ owner, name, branch: trimmedBranch, path: p, authToken: token })
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
              branch: trimmedBranch,
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
      const allEntries: Array<[string, string]> = [
        [storageKey(dataDir, 'connections'), JSON.stringify(connections)],
        [storageKey(dataDir, 'accounts'), JSON.stringify(accounts)],
        [manifestStorageKey(dataDir), JSON.stringify(manifestPaths)],
        ...fileEntries,
      ];

      for (let i = 0; i < allEntries.length; i += BATCH_SIZE) {
        await AsyncStorage.multiSet(allEntries.slice(i, i + BATCH_SIZE));
      }

      return '';
    } catch (e) {
      return String(e);
    }
  },
};

export default KeepbookNative;

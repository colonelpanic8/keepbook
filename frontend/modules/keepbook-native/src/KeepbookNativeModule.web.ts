import { registerWebModule, NativeModule } from 'expo';

import { ChangeEventPayload } from './KeepbookNative.types';

type KeepbookNativeModuleEvents = {
  onChange: (params: ChangeEventPayload) => void;
}

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

function nowRfc3339() {
  return new Date().toISOString();
}

function storageKey(dataDir: string, kind: 'connections' | 'accounts') {
  return `keepbook.web.${dataDir}.${kind}`;
}

function gitDataDirKey() {
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
  // Minimal TOML parsing for `key = "value"` fields; good enough for connection.toml config fields.
  const re = new RegExp(`^\\s*${key}\\s*=\\s*\"([^\"]*)\"\\s*$`, 'm');
  const m = toml.match(re);
  return m ? m[1] : null;
}

async function fetchJson(url: string, authToken?: string): Promise<any> {
  const headers: Record<string, string> = {
    // GitHub API requires a user-agent; browsers set one automatically, but keep Accept explicit.
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
  // Use GitHub Contents API for private repos (raw.githubusercontent.com won't work reliably with CORS+auth).
  // The raw Accept header returns file contents as plain text.
  const { owner, name, branch, path, authToken } = opts;
  const url =
    `https://api.github.com/repos/${encodeURIComponent(owner)}/${encodeURIComponent(
      name
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

class KeepbookNativeModule extends NativeModule<KeepbookNativeModuleEvents> {
  PI = Math.PI;
  async setValueAsync(value: string): Promise<void> {
    this.emit('onChange', { value });
  }
  hello() {
    return 'Hello world! 👋';
  }
  version() {
    return 'web';
  }
  demoDataDir() {
    return 'demo';
  }
  gitRepoDir() {
    return '';
  }
  gitDataDir() {
    return gitDataDirKey();
  }
  initDemo(_dataDir: string) {
    const dataDir = (_dataDir || 'demo').trim() || 'demo';

    try {
      const createdAt = nowRfc3339();

      const connections: ConnectionSummary[] = [
        {
          id: 'conn-demo',
          name: 'Demo Bank (web)',
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
          name: 'Demo Checking (web)',
          connection_id: 'conn-demo',
          created_at: createdAt,
          active: true,
        },
      ];

      localStorage.setItem(storageKey(dataDir, 'connections'), JSON.stringify(connections));
      localStorage.setItem(storageKey(dataDir, 'accounts'), JSON.stringify(accounts));
      return '';
    } catch (e) {
      return String(e);
    }
  }
  listConnections(_dataDir: string) {
    const dataDir = (_dataDir || 'demo').trim() || 'demo';
    try {
      const v = localStorage.getItem(storageKey(dataDir, 'connections'));
      return v || '[]';
    } catch (e) {
      return JSON.stringify({ error: String(e) });
    }
  }
  listAccounts(_dataDir: string) {
    const dataDir = (_dataDir || 'demo').trim() || 'demo';
    try {
      const v = localStorage.getItem(storageKey(dataDir, 'accounts'));
      return v || '[]';
    } catch (e) {
      return JSON.stringify({ error: String(e) });
    }
  }
  async gitSync(
    _repoDir: string,
    host: string,
    repo: string,
    _sshUser: string,
    _privateKeyPem: string,
    branch: string,
    authToken: string
  ) {
    const trimmedHost = host.trim();
    const parsed = parseOwnerRepo(repo);
    const trimmedBranch = (branch || '').trim() || 'main';

    if (!parsed) {
      return 'repo must be in the form owner/name';
    }

    // Start with GitHub-hosted repos over HTTP. Other hosts can be added later.
    if (trimmedHost !== 'github.com' && trimmedHost !== 'www.github.com') {
      return 'web sync currently supports only github.com';
    }

    const { owner, name } = parsed;
    const token = (authToken || '').trim();

    try {
      // 1) Discover paths via GitHub API (directory listing isn't available via raw).
      const treeUrl = `https://api.github.com/repos/${encodeURIComponent(owner)}/${encodeURIComponent(
        name
      )}/git/trees/${encodeURIComponent(trimmedBranch)}?recursive=1`;
      const tree = await fetchJson(treeUrl, token);

      const entries: Array<{ path?: string; type?: string }> = Array.isArray(tree?.tree) ? tree.tree : [];
      const connJsonPaths = entries
        .map((e) => e.path || '')
        .filter((p) => /^data\/connections\/[^/]+\/connection\.json$/.test(p));
      const connTomlPaths = new Set(
        entries
          .map((e) => e.path || '')
          .filter((p) => /^data\/connections\/[^/]+\/connection\.toml$/.test(p))
      );
      const acctJsonPaths = entries
        .map((e) => e.path || '')
        .filter((p) => /^data\/accounts\/[^/]+\/account\.json$/.test(p));

      const rawBase = `https://raw.githubusercontent.com/${encodeURIComponent(owner)}/${encodeURIComponent(
        name
      )}/${encodeURIComponent(trimmedBranch)}`;

      // 2) Fetch and summarize.
      const connections: ConnectionSummary[] = [];
      for (const p of connJsonPaths) {
        const id = p.split('/')[2] || '';
        const stateText = token
          ? await fetchGitHubText({ owner, name, branch: trimmedBranch, path: p, authToken: token })
          : await fetchText(`${rawBase}/${p}`);
        const state = JSON.parse(stateText);

        let cfgName = id;
        let cfgSync = 'unknown';
        const tomlPath = `data/connections/${id}/connection.toml`;
        if (connTomlPaths.has(tomlPath)) {
          const tomlText = token
            ? await fetchGitHubText({ owner, name, branch: trimmedBranch, path: tomlPath, authToken: token })
            : await fetchText(`${rawBase}/${tomlPath}`);
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
        const a = JSON.parse(acctText);
        accounts.push({
          id: String(a?.id ?? ''),
          name: String(a?.name ?? ''),
          connection_id: String(a?.connection_id ?? ''),
          created_at: String(a?.created_at ?? nowRfc3339()),
          active: Boolean(a?.active ?? true),
        });
      }

      localStorage.setItem(storageKey(gitDataDirKey(), 'connections'), JSON.stringify(connections));
      localStorage.setItem(storageKey(gitDataDirKey(), 'accounts'), JSON.stringify(accounts));
      return '';
    } catch (e) {
      return String(e);
    }
  }
};

export default registerWebModule(KeepbookNativeModule, 'KeepbookNative');

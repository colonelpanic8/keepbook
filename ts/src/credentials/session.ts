import * as fsSync from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

export interface SessionData {
  readonly token?: string | null;
  readonly cookies?: Record<string, string>;
  readonly captured_at?: number | null;
  readonly data?: Record<string, string>;
}

function defaultCacheDir(): string {
  const xdg = process.env.XDG_CACHE_HOME;
  if (xdg && xdg.trim() !== '') {
    return xdg;
  }
  return path.join(os.homedir(), '.cache');
}

function sessionsDir(): string {
  return path.join(defaultCacheDir(), 'keepbook', 'sessions');
}

function cookieHeader(cookies: Record<string, string>): string {
  return Object.entries(cookies)
    .map(([k, v]) => `${k}=${v}`)
    .join('; ');
}

export const SessionDataUtil = {
  cookieHeader(session: SessionData): string {
    return cookieHeader(session.cookies ?? {});
  },
} as const;

export class SessionCache {
  private readonly dir: string;

  private constructor(dir: string) {
    this.dir = dir;
  }

  static new(): SessionCache {
    const dir = sessionsDir();
    fsSync.mkdirSync(dir, { recursive: true });
    return new SessionCache(dir);
  }

  static withPath(dir: string): SessionCache {
    fsSync.mkdirSync(dir, { recursive: true });
    return new SessionCache(dir);
  }

  private sessionFile(connectionId: string): string {
    return path.join(this.dir, `${connectionId}.json`);
  }

  get(connectionId: string): SessionData | null {
    const file = this.sessionFile(connectionId);
    if (!fsSync.existsSync(file)) return null;
    const content = fsSync.readFileSync(file, 'utf8');
    return JSON.parse(content) as SessionData;
  }

  set(connectionId: string, session: SessionData): void {
    const file = this.sessionFile(connectionId);
    const content = JSON.stringify(session, null, 2) + '\n';
    fsSync.writeFileSync(file, content, 'utf8');
  }

  delete(connectionId: string): void {
    const file = this.sessionFile(connectionId);
    try {
      fsSync.unlinkSync(file);
    } catch {
      // ignore
    }
  }
}


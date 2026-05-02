import { execFile } from 'node:child_process';
import * as path from 'node:path';
import { promisify } from 'node:util';

import type { AgeConfig } from './credential-config.js';
import type { CredentialStore } from './credential-store.js';

const execFileAsync = promisify(execFile);
const IDENTITY_PATH_ENV = 'KEEPBOOK_CREDENTIALS_AGE_IDENTITY_PATH';

type FieldEntry = {
  fields: Map<string, string>;
};

function parseFieldEntry(content: string): FieldEntry {
  const lines = content.split(/\r?\n/);
  const fields = new Map<string, string>();
  if (lines.length > 0 && lines[0] !== '') {
    fields.set('password', lines[0]);
  }
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i];
    const idx = line.indexOf(': ');
    if (idx === -1) continue;
    fields.set(line.slice(0, idx), line.slice(idx + 2).replace(/\\n/g, '\n'));
  }
  return { fields };
}

export class AgeCredentialStore implements CredentialStore {
  private readonly config: AgeConfig;
  private readonly baseDir?: string;

  constructor(config: AgeConfig, baseDir?: string) {
    this.config = config;
    this.baseDir = baseDir;
  }

  private resolvePath(configured: string): string {
    if (path.isAbsolute(configured)) return configured;
    return this.baseDir !== undefined ? path.join(this.baseDir, configured) : configured;
  }

  private identityPath(): string {
    const configured = this.config.identity_path ?? process.env[IDENTITY_PATH_ENV];
    if (configured === undefined || configured.trim() === '') {
      throw new Error(`age credential identity is not configured; set identity_path or ${IDENTITY_PATH_ENV}`);
    }
    return this.resolvePath(configured.trim());
  }

  private fieldName(key: string): string {
    return this.config.fields[key] ?? key;
  }

  private async readEntry(): Promise<FieldEntry> {
    const agePath = this.resolvePath(this.config.path);
    const identityPath = this.identityPath();
    const { stdout } = await execFileAsync('age', ['--decrypt', '--identity', identityPath, agePath], {
      encoding: 'utf8',
      maxBuffer: 10 * 1024 * 1024,
    });
    return parseFieldEntry(stdout);
  }

  async get(key: string): Promise<string | null> {
    const entry = await this.readEntry();
    return entry.fields.get(this.fieldName(key)) ?? null;
  }

  async set(_key: string, _value: string): Promise<void> {
    throw new Error('age credential store is read-only');
  }

  supportsWrite(): boolean {
    return false;
  }
}

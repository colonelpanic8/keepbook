import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';

import type { CredentialStore } from './credential-store.js';
import type { PassConfig } from './credential-config.js';

const execFileAsync = promisify(execFile);

type PassEntry = {
  password: string | null;
  fields: Map<string, string>;
};

function parsePassEntry(content: string): PassEntry {
  const lines = content.split(/\r?\n/);
  const password = lines.length > 0 && lines[0] !== '' ? lines[0] : null;
  const fields = new Map<string, string>();

  if (password !== null) {
    fields.set('password', password);
  }

  for (let i = 1; i < lines.length; i++) {
    const line = lines[i];
    const idx = line.indexOf(': ');
    if (idx === -1) continue;
    const key = line.slice(0, idx);
    const value = line.slice(idx + 2).replace(/\\n/g, '\n');
    fields.set(key, value);
  }

  return { password, fields };
}

function serializePassEntry(entry: PassEntry): string {
  const lines: string[] = [];
  lines.push(entry.password ?? '');

  const keys = Array.from(entry.fields.keys()).filter((k) => k !== 'password');
  keys.sort((a, b) => a.localeCompare(b));
  for (const key of keys) {
    const value = entry.fields.get(key);
    if (value === undefined) continue;
    lines.push(`${key}: ${value.replace(/\n/g, '\\n')}`);
  }

  return lines.join('\n') + '\n';
}

/**
 * Credential store backed by password-store (pass).
 *
 * Mirrors the Rust `PassCredentialStore` behavior:
 * - `pass show <path>` reads the entry.
 * - First line is treated as "password" and also available under key "password".
 * - Other lines parsed as `field: value`, with `\\n` unescaped in values.
 */
export class PassCredentialStore implements CredentialStore {
  private readonly config: PassConfig;

  constructor(config: PassConfig) {
    this.config = config;
  }

  private fieldName(key: string): string {
    return this.config.fields[key] ?? key;
  }

  private async readEntry(): Promise<PassEntry> {
    const { stdout } = await execFileAsync('pass', ['show', this.config.path], {
      encoding: 'utf8',
      maxBuffer: 10 * 1024 * 1024,
    });
    return parsePassEntry(stdout);
  }

  private async writeEntry(entry: PassEntry): Promise<void> {
    const content = serializePassEntry(entry);
    await new Promise<void>((resolve, reject) => {
      const child = spawn('pass', ['insert', '--multiline', '--force', this.config.path], {
        stdio: ['pipe', 'ignore', 'pipe'],
      });

      let stderr = '';
      child.stderr.setEncoding('utf8');
      child.stderr.on('data', (chunk) => {
        stderr += chunk;
      });

      child.on('error', (err) => reject(err));
      child.on('close', (code) => {
        if (code === 0) resolve();
        reject(new Error(`pass insert failed (code ${code}): ${stderr.trim()}`));
      });

      child.stdin.setDefaultEncoding('utf8');
      child.stdin.write(content);
      child.stdin.end();
    });
  }

  async get(key: string): Promise<string | null> {
    const field = this.fieldName(key);
    const entry = await this.readEntry();
    return entry.fields.get(field) ?? null;
  }

  async set(key: string, value: string): Promise<void> {
    const field = this.fieldName(key);

    let entry: PassEntry;
    try {
      entry = await this.readEntry();
    } catch {
      entry = { password: null, fields: new Map<string, string>() };
    }

    // Maintain password line separately if caller sets "password".
    if (field === 'password') {
      entry.password = value;
      entry.fields.set('password', value);
    } else {
      entry.fields.set(field, value);
    }

    await this.writeEntry(entry);
  }

  supportsWrite(): boolean {
    return true;
  }
}


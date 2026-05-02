import type { CredentialStore } from './credential-store.js';
import type { EnvConfig } from './credential-config.js';

export class EnvCredentialStore implements CredentialStore {
  private readonly config: EnvConfig;

  constructor(config: EnvConfig) {
    this.config = config;
  }

  private envName(key: string): string {
    const mapped = this.config.fields[key];
    if (mapped !== undefined) return mapped;
    const normalized = key.replace(/[^A-Za-z0-9]/g, '_').toUpperCase();
    return `${this.config.prefix ?? ''}${normalized}`;
  }

  async get(key: string): Promise<string | null> {
    const value = process.env[this.envName(key)];
    return value !== undefined && value !== '' ? value : null;
  }

  async set(_key: string, _value: string): Promise<void> {
    throw new Error('environment credential store is read-only');
  }

  supportsWrite(): boolean {
    return false;
  }
}

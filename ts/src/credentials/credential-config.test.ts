import { describe, it, expect } from 'vitest';
import { parseCredentialConfig, CredentialConfig } from './credential-config.js';
import { CredentialStore } from './credential-store.js';

describe('parseCredentialConfig', () => {
  it('parses TOML with pass backend, path, and fields', () => {
    const toml = `
backend = "pass"
path = "finance/coinbase-api"

[fields]
key_name = "key-name"
private_key = "private-key"
`;
    const config = parseCredentialConfig(toml);
    expect(config).toEqual({
      backend: 'pass',
      path: 'finance/coinbase-api',
      fields: {
        key_name: 'key-name',
        private_key: 'private-key',
      },
    });
  });

  it('parses minimal TOML (just backend and path, no fields section)', () => {
    const toml = `
backend = "pass"
path = "finance/kraken"
`;
    const config = parseCredentialConfig(toml);
    expect(config).toEqual({
      backend: 'pass',
      path: 'finance/kraken',
      fields: {},
    });
  });

  it('throws on unknown backend', () => {
    const toml = `
backend = "vault"
path = "some/path"
`;
    expect(() => parseCredentialConfig(toml)).toThrow();
  });

  it('throws on missing backend', () => {
    const toml = `
path = "some/path"
`;
    expect(() => parseCredentialConfig(toml)).toThrow();
  });

  it('throws on missing path', () => {
    const toml = `
backend = "pass"
`;
    expect(() => parseCredentialConfig(toml)).toThrow();
  });
});

describe('CredentialConfig JSON round-trip', () => {
  it('serializes and deserializes CredentialConfig correctly', () => {
    const config: CredentialConfig = {
      backend: 'pass',
      path: 'finance/coinbase-api',
      fields: {
        key_name: 'key-name',
        private_key: 'private-key',
      },
    };

    const json = JSON.stringify(config);
    const parsed: CredentialConfig = JSON.parse(json);

    expect(parsed).toEqual(config);
    expect(parsed.backend).toBe('pass');
    expect(parsed.path).toBe('finance/coinbase-api');
    expect(parsed.fields).toEqual({
      key_name: 'key-name',
      private_key: 'private-key',
    });
  });

  it('round-trips a config with empty fields', () => {
    const config: CredentialConfig = {
      backend: 'pass',
      path: 'finance/kraken',
      fields: {},
    };

    const json = JSON.stringify(config);
    const parsed: CredentialConfig = JSON.parse(json);

    expect(parsed).toEqual(config);
  });
});

describe('CredentialStore interface', () => {
  it('can be implemented as a mock and supports get/set/supportsWrite', async () => {
    const store: Record<string, string> = {};
    const mock: CredentialStore = {
      async get(key: string): Promise<string | null> {
        return store[key] ?? null;
      },
      async set(key: string, value: string): Promise<void> {
        store[key] = value;
      },
      supportsWrite(): boolean {
        return true;
      },
    };

    // Initially empty
    expect(await mock.get('api-key')).toBeNull();

    // Set and retrieve
    await mock.set('api-key', 'secret-123');
    expect(await mock.get('api-key')).toBe('secret-123');

    // Overwrite
    await mock.set('api-key', 'secret-456');
    expect(await mock.get('api-key')).toBe('secret-456');

    // supportsWrite
    expect(mock.supportsWrite()).toBe(true);
  });

  it('can implement a read-only store', async () => {
    const readOnly: CredentialStore = {
      async get(_key: string): Promise<string | null> {
        return 'read-only-value';
      },
      async set(_key: string, _value: string): Promise<void> {
        throw new Error('Read-only store');
      },
      supportsWrite(): boolean {
        return false;
      },
    };

    expect(readOnly.supportsWrite()).toBe(false);
    expect(await readOnly.get('anything')).toBe('read-only-value');
    await expect(readOnly.set('key', 'val')).rejects.toThrow('Read-only store');
  });
});

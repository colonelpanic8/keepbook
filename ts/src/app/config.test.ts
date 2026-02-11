import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';

import { defaultConfigPath, loadConfig, configOutput } from './config.js';
import { DEFAULT_REFRESH_CONFIG } from '../config.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Create a unique temp directory that is cleaned up after the test. */
function makeTmpDir(): string {
  const dir = path.join(os.tmpdir(), `keepbook-test-${crypto.randomUUID()}`);
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

// ---------------------------------------------------------------------------
// defaultConfigPath
// ---------------------------------------------------------------------------

describe('defaultConfigPath', () => {
  it('returns a string', () => {
    const result = defaultConfigPath();
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
  });

  it('returns an absolute path', () => {
    const result = defaultConfigPath();
    expect(path.isAbsolute(result)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// loadConfig – with a real TOML file
// ---------------------------------------------------------------------------

describe('loadConfig', () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = makeTmpDir();
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it('reads and parses a TOML config file', async () => {
    const tomlContent = [
      'reporting_currency = "EUR"',
      '',
      '[git]',
      'auto_commit = true',
      '',
      '[refresh]',
      'balance_staleness = "7d"',
      'price_staleness = "12h"',
    ].join('\n');

    const configFile = path.join(tmpDir, 'keepbook.toml');
    fs.writeFileSync(configFile, tomlContent, 'utf-8');

    const { configPath, config } = await loadConfig(configFile);

    expect(configPath).toBe(configFile);
    expect(config.reporting_currency).toBe('EUR');
    expect(config.git.auto_commit).toBe(true);
    expect(config.refresh.balance_staleness).toBe(7 * 24 * 60 * 60 * 1000);
    expect(config.refresh.price_staleness).toBe(12 * 60 * 60 * 1000);
  });

  it('resolves data_dir relative to config file parent', async () => {
    const tomlContent = 'data_dir = "mydata"';
    const configFile = path.join(tmpDir, 'keepbook.toml');
    fs.writeFileSync(configFile, tomlContent, 'utf-8');

    const { config } = await loadConfig(configFile);

    expect(config.data_dir).toBe(path.join(tmpDir, 'mydata'));
  });

  it('keeps absolute data_dir as-is', async () => {
    const absDataDir = path.join(tmpDir, 'absolute-data');
    const tomlContent = `data_dir = "${absDataDir.replace(/\\/g, '\\\\')}"`;
    const configFile = path.join(tmpDir, 'keepbook.toml');
    fs.writeFileSync(configFile, tomlContent, 'utf-8');

    const { config } = await loadConfig(configFile);

    expect(config.data_dir).toBe(absDataDir);
  });

  it('uses config file parent as data_dir when data_dir is omitted', async () => {
    const configFile = path.join(tmpDir, 'keepbook.toml');
    fs.writeFileSync(configFile, '', 'utf-8');

    const { config } = await loadConfig(configFile);

    expect(config.data_dir).toBe(tmpDir);
  });

  it('returns defaults when config file does not exist', async () => {
    const nonExistent = path.join(tmpDir, 'nope', 'keepbook.toml');

    const { configPath, config } = await loadConfig(nonExistent);

    expect(configPath).toBe(nonExistent);
    expect(config.reporting_currency).toBe('USD');
    expect(config.git.auto_commit).toBe(false);
    expect(config.refresh.balance_staleness).toBe(DEFAULT_REFRESH_CONFIG.balance_staleness);
    expect(config.refresh.price_staleness).toBe(DEFAULT_REFRESH_CONFIG.price_staleness);
    // data_dir falls back to the intended config directory
    expect(config.data_dir).toBe(path.dirname(nonExistent));
  });

  it('returns defaults when no configPath argument is given (no file on disk)', async () => {
    // We cannot easily control defaultConfigPath, but we can verify it
    // produces a valid result regardless of whether a file exists.
    const { configPath, config } = await loadConfig();

    expect(typeof configPath).toBe('string');
    expect(path.isAbsolute(configPath)).toBe(true);
    expect(config.reporting_currency).toBeDefined();
    expect(config.git).toBeDefined();
    expect(config.refresh).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// configOutput – JSON shape
// ---------------------------------------------------------------------------

describe('configOutput', () => {
  it('produces exact JSON shape matching Rust', () => {
    const result = configOutput('/path/to/keepbook.toml', {
      data_dir: '/path/to/data',
      reporting_currency: 'USD',
      refresh: { ...DEFAULT_REFRESH_CONFIG },
      git: { auto_commit: false },
    });

    expect(result).toEqual({
      config_file: '/path/to/keepbook.toml',
      data_directory: '/path/to/data',
      git: {
        auto_commit: false,
      },
    });
  });

  it('reflects auto_commit = true when set', () => {
    const result = configOutput('/some/file.toml', {
      data_dir: '/data',
      reporting_currency: 'EUR',
      refresh: { ...DEFAULT_REFRESH_CONFIG },
      git: { auto_commit: true },
    });

    expect(result).toEqual({
      config_file: '/some/file.toml',
      data_directory: '/data',
      git: {
        auto_commit: true,
      },
    });
  });

  it('does not include extra fields like reporting_currency or refresh', () => {
    const result = configOutput('/f.toml', {
      data_dir: '/d',
      reporting_currency: 'GBP',
      refresh: { balance_staleness: 1, price_staleness: 2 },
      git: { auto_commit: false },
    }) as Record<string, unknown>;

    expect(Object.keys(result).sort()).toEqual(['config_file', 'data_directory', 'git'].sort());
  });
});

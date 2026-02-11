import { describe, expect, it } from 'vitest';
import {
  DEFAULT_REFRESH_CONFIG,
  DEFAULT_GIT_CONFIG,
  DEFAULT_CONFIG,
  parseConfig,
  resolveDataDir,
} from './config.js';
import type { Config } from './config.js';

const MS_PER_DAY = 24 * 60 * 60 * 1000;
const MS_PER_HOUR = 60 * 60 * 1000;

describe('default constants', () => {
  describe('DEFAULT_REFRESH_CONFIG', () => {
    it('has balance_staleness of 14 days in ms', () => {
      expect(DEFAULT_REFRESH_CONFIG.balance_staleness).toBe(14 * MS_PER_DAY);
    });

    it('has price_staleness of 24 hours in ms', () => {
      expect(DEFAULT_REFRESH_CONFIG.price_staleness).toBe(24 * MS_PER_HOUR);
    });
  });

  describe('DEFAULT_GIT_CONFIG', () => {
    it('has auto_commit false', () => {
      expect(DEFAULT_GIT_CONFIG.auto_commit).toBe(false);
    });

    it('has auto_push false', () => {
      expect(DEFAULT_GIT_CONFIG.auto_push).toBe(false);
    });
  });

  describe('DEFAULT_CONFIG', () => {
    it('has reporting_currency USD', () => {
      expect(DEFAULT_CONFIG.reporting_currency).toBe('USD');
    });

    it('has no data_dir', () => {
      expect(DEFAULT_CONFIG.data_dir).toBeUndefined();
    });

    it('has default refresh config', () => {
      expect(DEFAULT_CONFIG.refresh).toEqual(DEFAULT_REFRESH_CONFIG);
    });

    it('has default git config', () => {
      expect(DEFAULT_CONFIG.git).toEqual(DEFAULT_GIT_CONFIG);
    });
  });
});

describe('parseConfig', () => {
  it('returns defaults for empty string', () => {
    const config = parseConfig('');
    expect(config.data_dir).toBeUndefined();
    expect(config.reporting_currency).toBe('USD');
    expect(config.refresh.balance_staleness).toBe(14 * MS_PER_DAY);
    expect(config.refresh.price_staleness).toBe(24 * MS_PER_HOUR);
    expect(config.git.auto_commit).toBe(false);
    expect(config.git.auto_push).toBe(false);
  });

  it('parses data_dir only', () => {
    const config = parseConfig('data_dir = "./my-data"');
    expect(config.data_dir).toBe('./my-data');
    expect(config.reporting_currency).toBe('USD');
    expect(config.refresh).toEqual(DEFAULT_REFRESH_CONFIG);
    expect(config.git).toEqual(DEFAULT_GIT_CONFIG);
  });

  it('parses reporting_currency', () => {
    const config = parseConfig('reporting_currency = "EUR"');
    expect(config.reporting_currency).toBe('EUR');
  });

  it('parses refresh config with duration strings', () => {
    const toml = `
[refresh]
balance_staleness = "7d"
price_staleness = "1h"
`;
    const config = parseConfig(toml);
    expect(config.refresh.balance_staleness).toBe(7 * MS_PER_DAY);
    expect(config.refresh.price_staleness).toBe(1 * MS_PER_HOUR);
  });

  it('uses defaults for missing refresh fields', () => {
    const toml = `
[refresh]
balance_staleness = "7d"
`;
    const config = parseConfig(toml);
    expect(config.refresh.balance_staleness).toBe(7 * MS_PER_DAY);
    expect(config.refresh.price_staleness).toBe(24 * MS_PER_HOUR);
  });

  it('parses git config', () => {
    const toml = `
[git]
auto_commit = true
auto_push = true
`;
    const config = parseConfig(toml);
    expect(config.git.auto_commit).toBe(true);
    expect(config.git.auto_push).toBe(true);
  });

  it('defaults auto_push to false when omitted', () => {
    const toml = `
[git]
auto_commit = true
`;
    const config = parseConfig(toml);
    expect(config.git.auto_commit).toBe(true);
    expect(config.git.auto_push).toBe(false);
  });

  it('parses full config', () => {
    const toml = `
data_dir = "./my-data"
reporting_currency = "EUR"

[refresh]
balance_staleness = "7d"
price_staleness = "1h"

[git]
auto_commit = true
auto_push = true
`;
    const config = parseConfig(toml);
    expect(config.data_dir).toBe('./my-data');
    expect(config.reporting_currency).toBe('EUR');
    expect(config.refresh.balance_staleness).toBe(7 * MS_PER_DAY);
    expect(config.refresh.price_staleness).toBe(1 * MS_PER_HOUR);
    expect(config.git.auto_commit).toBe(true);
    expect(config.git.auto_push).toBe(true);
  });
});

describe('resolveDataDir', () => {
  it('returns configDir when no data_dir is set', () => {
    const config: Config = { ...DEFAULT_CONFIG };
    expect(resolveDataDir(config, '/home/user/.config/keepbook')).toBe(
      '/home/user/.config/keepbook',
    );
  });

  it('joins relative data_dir with configDir', () => {
    const config: Config = { ...DEFAULT_CONFIG, data_dir: './my-data' };
    const result = resolveDataDir(config, '/home/user/.config/keepbook');
    expect(result).toBe('/home/user/.config/keepbook/my-data');
  });

  it('joins bare relative data_dir with configDir', () => {
    const config: Config = { ...DEFAULT_CONFIG, data_dir: 'data' };
    const result = resolveDataDir(config, '/home/user/.config/keepbook');
    expect(result).toBe('/home/user/.config/keepbook/data');
  });

  it('uses absolute data_dir directly', () => {
    const config: Config = { ...DEFAULT_CONFIG, data_dir: '/opt/keepbook/data' };
    const result = resolveDataDir(config, '/home/user/.config/keepbook');
    expect(result).toBe('/opt/keepbook/data');
  });
});

import { describe, expect, it } from 'vitest';
import {
  DEFAULT_REFRESH_CONFIG,
  DEFAULT_GIT_CONFIG,
  DEFAULT_TRAY_CONFIG,
  DEFAULT_SPENDING_CONFIG,
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

    it('has merge_master_before_command false', () => {
      expect(DEFAULT_GIT_CONFIG.merge_master_before_command).toBe(false);
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

    it('has default tray config', () => {
      expect(DEFAULT_CONFIG.tray).toEqual(DEFAULT_TRAY_CONFIG);
    });

    it('has default spending config', () => {
      expect(DEFAULT_CONFIG.spending).toEqual(DEFAULT_SPENDING_CONFIG);
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
    expect(config.git.merge_master_before_command).toBe(false);
    expect(config.tray).toEqual(DEFAULT_TRAY_CONFIG);
    expect(config.spending).toEqual(DEFAULT_SPENDING_CONFIG);
  });

  it('parses display currency formatting options', () => {
    const toml = `
[display]
currency_grouping = true
currency_symbol = "$"
currency_fixed_decimals = true
currency_decimals = 2
`;
    const config = parseConfig(toml);
    expect(config.display.currency_grouping).toBe(true);
    expect(config.display.currency_symbol).toBe('$');
    expect(config.display.currency_fixed_decimals).toBe(true);
    expect(config.display.currency_decimals).toBe(2);
  });

  it('parses data_dir only', () => {
    const config = parseConfig('data_dir = "./my-data"');
    expect(config.data_dir).toBe('./my-data');
    expect(config.reporting_currency).toBe('USD');
    expect(config.refresh).toEqual(DEFAULT_REFRESH_CONFIG);
    expect(config.tray).toEqual(DEFAULT_TRAY_CONFIG);
    expect(config.spending).toEqual(DEFAULT_SPENDING_CONFIG);
    expect(config.git).toEqual(DEFAULT_GIT_CONFIG);
  });

  it('parses tray config', () => {
    const toml = `
[tray]
history_points = 5
spending_windows_days = [3, 14, 60]
`;
    const config = parseConfig(toml);
    expect(config.tray.history_points).toBe(5);
    expect(config.tray.spending_windows_days).toEqual([3, 14, 60]);
  });

  it('parses spending ignore config', () => {
    const toml = `
[spending]
ignore_accounts = ["Individual", "acct-1"]
ignore_connections = ["Schwab"]
ignore_tags = ["brokerage"]
`;
    const config = parseConfig(toml);
    expect(config.spending.ignore_accounts).toEqual(['Individual', 'acct-1']);
    expect(config.spending.ignore_connections).toEqual(['Schwab']);
    expect(config.spending.ignore_tags).toEqual(['brokerage']);
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
merge_master_before_command = true
`;
    const config = parseConfig(toml);
    expect(config.git.auto_commit).toBe(true);
    expect(config.git.auto_push).toBe(true);
    expect(config.git.merge_master_before_command).toBe(true);
  });

  it('defaults auto_push to false when omitted', () => {
    const toml = `
[git]
auto_commit = true
`;
    const config = parseConfig(toml);
    expect(config.git.auto_commit).toBe(true);
    expect(config.git.auto_push).toBe(false);
    expect(config.git.merge_master_before_command).toBe(false);
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
merge_master_before_command = true
`;
    const config = parseConfig(toml);
    expect(config.data_dir).toBe('./my-data');
    expect(config.reporting_currency).toBe('EUR');
    expect(config.refresh.balance_staleness).toBe(7 * MS_PER_DAY);
    expect(config.refresh.price_staleness).toBe(1 * MS_PER_HOUR);
    expect(config.git.auto_commit).toBe(true);
    expect(config.git.auto_push).toBe(true);
    expect(config.git.merge_master_before_command).toBe(true);
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
    expect(result).toBe('/home/user/.config/keepbook/./my-data');
  });

  it('joins bare relative data_dir with configDir', () => {
    const config: Config = { ...DEFAULT_CONFIG, data_dir: 'data' };
    const result = resolveDataDir(config, '/home/user/.config/keepbook');
    expect(result).toBe('/home/user/.config/keepbook/data');
  });

  it('preserves dot segment for data_dir "." to match Rust PathBuf join behavior', () => {
    const config: Config = { ...DEFAULT_CONFIG, data_dir: '.' };
    const result = resolveDataDir(config, '/home/user/.config/keepbook');
    expect(result).toBe('/home/user/.config/keepbook/.');
  });

  it('uses absolute data_dir directly', () => {
    const config: Config = { ...DEFAULT_CONFIG, data_dir: '/opt/keepbook/data' };
    const result = resolveDataDir(config, '/home/user/.config/keepbook');
    expect(result).toBe('/opt/keepbook/data');
  });
});

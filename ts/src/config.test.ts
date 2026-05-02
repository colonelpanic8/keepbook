import { describe, expect, it } from 'vitest';
import os from 'node:os';
import path from 'node:path';
import {
  DEFAULT_REFRESH_CONFIG,
  DEFAULT_GIT_CONFIG,
  DEFAULT_TRAY_CONFIG,
  DEFAULT_HISTORY_CONFIG,
  DEFAULT_SPENDING_CONFIG,
  DEFAULT_PORTFOLIO_CONFIG,
  DEFAULT_IGNORE_CONFIG,
  DEFAULT_CONFIG,
  expandTildePath,
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

    it('has pull_before_edit false', () => {
      expect(DEFAULT_GIT_CONFIG.pull_before_edit).toBe(false);
    });

    it('has push_after_sync false', () => {
      expect(DEFAULT_GIT_CONFIG.push_after_sync).toBe(false);
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

    it('has default history config', () => {
      expect(DEFAULT_CONFIG.history).toEqual(DEFAULT_HISTORY_CONFIG);
    });

    it('has default spending config', () => {
      expect(DEFAULT_CONFIG.spending).toEqual(DEFAULT_SPENDING_CONFIG);
    });

    it('has default portfolio config', () => {
      expect(DEFAULT_CONFIG.portfolio).toEqual(DEFAULT_PORTFOLIO_CONFIG);
    });

    it('has default ignore config', () => {
      expect(DEFAULT_CONFIG.ignore).toEqual(DEFAULT_IGNORE_CONFIG);
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
    expect(config.git.pull_before_edit).toBe(false);
    expect(config.git.push_after_sync).toBe(false);
    expect(config.git.merge_master_before_command).toBe(false);
    expect(config.tray).toEqual(DEFAULT_TRAY_CONFIG);
    expect(config.history).toEqual(DEFAULT_HISTORY_CONFIG);
    expect(config.spending).toEqual(DEFAULT_SPENDING_CONFIG);
    expect(config.portfolio).toEqual(DEFAULT_PORTFOLIO_CONFIG);
    expect(config.ignore).toEqual(DEFAULT_IGNORE_CONFIG);
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
    expect(config.history).toEqual(DEFAULT_HISTORY_CONFIG);
    expect(config.spending).toEqual(DEFAULT_SPENDING_CONFIG);
    expect(config.ignore).toEqual(DEFAULT_IGNORE_CONFIG);
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

  it('parses history config', () => {
    const toml = `
[history]
allow_future_projection = true
lookback_days = 7
portfolio_granularity = "weekly"
change_points_granularity = "daily"
include_prices = false
graph_range = "2y"
graph_granularity = "monthly"
`;
    const config = parseConfig(toml);
    expect(config.history).toEqual({
      ...DEFAULT_HISTORY_CONFIG,
      allow_future_projection: true,
      lookback_days: 7,
      portfolio_granularity: 'weekly',
      change_points_granularity: 'daily',
      include_prices: false,
      graph_range: '2y',
      graph_granularity: 'monthly',
    });
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

  it('parses latent capital gains tax portfolio config', () => {
    const toml = `
[portfolio.latent_capital_gains_tax]
enabled = true
rate = 0.23
account_name = "Estimated Tax Liability"
`;
    const config = parseConfig(toml);
    expect(config.portfolio.latent_capital_gains_tax).toEqual({
      enabled: true,
      rate: 0.23,
      account_name: 'Estimated Tax Liability',
    });
  });

  it('parses global ignore transaction rules', () => {
    const toml = `
[ignore]
[[ignore.transaction_rules]]
account_name = "(?i)^Investor Checking$"
connection_name = "(?i)^Charles Schwab$"
synchronizer = "(?i)^schwab$"
description = "(?i)credit\\\\s+crd\\\\s+(?:e?pay|autopay)"
`;
    const config = parseConfig(toml);
    expect(config.ignore.transaction_rules).toEqual([
      {
        account_name: '(?i)^Investor Checking$',
        connection_name: '(?i)^Charles Schwab$',
        synchronizer: '(?i)^schwab$',
        description: '(?i)credit\\s+crd\\s+(?:e?pay|autopay)',
      },
    ]);
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
pull_before_edit = true
push_after_sync = true
merge_master_before_command = true
`;
    const config = parseConfig(toml);
    expect(config.git.auto_commit).toBe(true);
    expect(config.git.auto_push).toBe(true);
    expect(config.git.pull_before_edit).toBe(true);
    expect(config.git.push_after_sync).toBe(true);
    expect(config.git.merge_master_before_command).toBe(true);
  });

  it('defaults auto_push to auto_commit when omitted', () => {
    const toml = `
[git]
auto_commit = true
`;
    const config = parseConfig(toml);
    expect(config.git.auto_commit).toBe(true);
    expect(config.git.auto_push).toBe(true);
    expect(config.git.pull_before_edit).toBe(false);
    expect(config.git.push_after_sync).toBe(false);
    expect(config.git.merge_master_before_command).toBe(false);
  });

  it('allows disabling auto_push while auto_commit is enabled', () => {
    const toml = `
[git]
auto_commit = true
auto_push = false
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
pull_before_edit = true
push_after_sync = true
merge_master_before_command = true
`;
    const config = parseConfig(toml);
    expect(config.data_dir).toBe('./my-data');
    expect(config.reporting_currency).toBe('EUR');
    expect(config.refresh.balance_staleness).toBe(7 * MS_PER_DAY);
    expect(config.refresh.price_staleness).toBe(1 * MS_PER_HOUR);
    expect(config.git.auto_commit).toBe(true);
    expect(config.git.auto_push).toBe(true);
    expect(config.git.pull_before_edit).toBe(true);
    expect(config.git.push_after_sync).toBe(true);
    expect(config.git.merge_master_before_command).toBe(true);
  });
});

describe('resolveDataDir', () => {
  it('expands bare tilde paths to the home directory', () => {
    expect(expandTildePath('~')).toBe(os.homedir());
    expect(expandTildePath('~/keepbook.toml')).toBe(path.join(os.homedir(), 'keepbook.toml'));
    expect(expandTildePath('~other/keepbook.toml')).toBe('~other/keepbook.toml');
  });

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

  it('expands tilde data_dir before resolving relative paths', () => {
    const config: Config = { ...DEFAULT_CONFIG, data_dir: '~/keepbook-data' };
    const result = resolveDataDir(config, '/home/user/.config/keepbook');
    expect(result).toBe(path.join(os.homedir(), 'keepbook-data'));
  });
});

/**
 * Configuration module for keepbook.
 *
 * Parses TOML configuration and provides sensible defaults.
 * Duration fields in the [refresh] section are human-readable strings
 * (e.g. "14d", "24h") that get converted to milliseconds.
 */

import path from 'node:path';
import toml from 'toml';
import { parseDuration } from './duration.js';

// ---------------------------------------------------------------------------
// Interfaces
// ---------------------------------------------------------------------------

export interface RefreshConfig {
  /** How long before a balance is considered stale (ms). */
  balance_staleness: number;
  /** How long before a price is considered stale (ms). */
  price_staleness: number;
}

export interface GitConfig {
  /** Whether to auto-commit data changes. */
  auto_commit: boolean;
  /** Whether to auto-push after successful auto-commits. */
  auto_push: boolean;
  /** Whether to merge origin/master before each command. */
  merge_master_before_command: boolean;
}

export interface Config {
  /** Optional path to the data directory. */
  data_dir?: string;
  /** Currency code used for reporting. */
  reporting_currency: string;
  /** Refresh / staleness settings. */
  refresh: RefreshConfig;
  /** Git integration settings. */
  git: GitConfig;
}

export interface ResolvedConfig {
  /** Resolved (absolute) path to the data directory. */
  data_dir: string;
  /** Currency code used for reporting. */
  reporting_currency: string;
  /** Refresh / staleness settings. */
  refresh: RefreshConfig;
  /** Git integration settings. */
  git: GitConfig;
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

const MS_PER_HOUR = 60 * 60 * 1000;
const MS_PER_DAY = 24 * MS_PER_HOUR;

export const DEFAULT_REFRESH_CONFIG: RefreshConfig = {
  balance_staleness: 14 * MS_PER_DAY,
  price_staleness: 24 * MS_PER_HOUR,
};

export const DEFAULT_GIT_CONFIG: GitConfig = {
  auto_commit: false,
  auto_push: false,
  merge_master_before_command: false,
};

export const DEFAULT_CONFIG: Config = {
  data_dir: undefined,
  reporting_currency: 'USD',
  refresh: { ...DEFAULT_REFRESH_CONFIG },
  git: { ...DEFAULT_GIT_CONFIG },
};

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/**
 * Parse a TOML configuration string into a `Config`.
 *
 * Missing fields are filled with defaults. Duration strings in the `[refresh]`
 * section (e.g. `"14d"`, `"24h"`) are converted to milliseconds via
 * {@link parseDuration}.
 */
export function parseConfig(tomlStr: string): Config {
  // toml.parse throws on invalid TOML; an empty string yields an empty object.
  const raw: Record<string, unknown> =
    tomlStr.trim().length === 0 ? {} : (toml.parse(tomlStr) as Record<string, unknown>);

  const refreshRaw = (raw.refresh ?? {}) as Record<string, unknown>;
  const gitRaw = (raw.git ?? {}) as Record<string, unknown>;

  const refresh: RefreshConfig = {
    balance_staleness:
      typeof refreshRaw.balance_staleness === 'string'
        ? parseDuration(refreshRaw.balance_staleness)
        : DEFAULT_REFRESH_CONFIG.balance_staleness,
    price_staleness:
      typeof refreshRaw.price_staleness === 'string'
        ? parseDuration(refreshRaw.price_staleness)
        : DEFAULT_REFRESH_CONFIG.price_staleness,
  };

  const git: GitConfig = {
    auto_commit:
      typeof gitRaw.auto_commit === 'boolean' ? gitRaw.auto_commit : DEFAULT_GIT_CONFIG.auto_commit,
    auto_push:
      typeof gitRaw.auto_push === 'boolean' ? gitRaw.auto_push : DEFAULT_GIT_CONFIG.auto_push,
    merge_master_before_command:
      typeof gitRaw.merge_master_before_command === 'boolean'
        ? gitRaw.merge_master_before_command
        : DEFAULT_GIT_CONFIG.merge_master_before_command,
  };

  const config: Config = {
    reporting_currency:
      typeof raw.reporting_currency === 'string'
        ? raw.reporting_currency
        : DEFAULT_CONFIG.reporting_currency,
    refresh,
    git,
  };

  if (typeof raw.data_dir === 'string') {
    config.data_dir = raw.data_dir;
  }

  return config;
}

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/**
 * Resolve the data directory for a config.
 *
 * - If `config.data_dir` is set and absolute, return it directly.
 * - If `config.data_dir` is set and relative, join it with `configDir`.
 * - If `config.data_dir` is not set, return `configDir`.
 */
export function resolveDataDir(config: Config, configDir: string): string {
  if (config.data_dir == null) {
    return configDir;
  }

  if (path.isAbsolute(config.data_dir)) {
    return config.data_dir;
  }

  // Match Rust `PathBuf::join` display semantics (preserve relative segments).
  if (configDir.endsWith(path.sep)) {
    return `${configDir}${config.data_dir}`;
  }
  return `${configDir}${path.sep}${config.data_dir}`;
}

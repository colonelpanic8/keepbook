/**
 * CLI-specific config loading utilities.
 *
 * Wraps the library-level config parsing (`../config.js`) with file-system
 * awareness: locating the config file, reading TOML, resolving paths, and
 * producing the JSON output shape expected by the `config` command.
 */

import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';

import {
  type Config,
  type ResolvedConfig,
  expandTildePath,
  parseConfig,
  resolveDataDir,
  DEFAULT_CONFIG,
  DEFAULT_GIT_CONFIG,
  DEFAULT_HISTORY_CONFIG,
  DEFAULT_REFRESH_CONFIG,
  DEFAULT_SPENDING_CONFIG,
  DEFAULT_PORTFOLIO_CONFIG,
  DEFAULT_IGNORE_CONFIG,
  DEFAULT_AI_CONFIG,
  DEFAULT_TRAY_CONFIG,
} from '../config.js';

// ---------------------------------------------------------------------------
// Default config path discovery
// ---------------------------------------------------------------------------

/**
 * Determine the default configuration file path.
 *
 * Resolution order:
 * 1. `./keepbook.toml` if it exists in the current working directory.
 * 2. `$XDG_DATA_HOME/keepbook/keepbook.toml` (or
 *    `~/.local/share/keepbook/keepbook.toml` when `XDG_DATA_HOME` is not set)
 *    if it exists.
 * 3. The XDG path from step 2 (even if it does not yet exist) as the default.
 */
export function defaultConfigPath(): string {
  const localConfig = path.resolve('keepbook.toml');
  if (fs.existsSync(localConfig)) {
    return localConfig;
  }

  const configuredXdgDataHome =
    process.env.XDG_DATA_HOME || path.join(os.homedir(), '.local', 'share');
  const expandedXdgDataHome = expandTildePath(configuredXdgDataHome);
  const xdgDataHome = path.isAbsolute(expandedXdgDataHome)
    ? expandedXdgDataHome
    : path.resolve(expandedXdgDataHome);
  const xdgConfig = path.join(xdgDataHome, 'keepbook', 'keepbook.toml');
  if (fs.existsSync(xdgConfig)) {
    return xdgConfig;
  }

  // Return XDG default even when nothing exists on disk yet.
  return xdgConfig;
}

// ---------------------------------------------------------------------------
// Config loading
// ---------------------------------------------------------------------------

/**
 * Load and resolve the keepbook configuration.
 *
 * @param configPath - Explicit path to a TOML config file. When omitted the
 *   result of {@link defaultConfigPath} is used.
 * @returns The resolved path that was used and the fully-resolved config.
 */
export async function loadConfig(
  configPath?: string,
): Promise<{ configPath: string; config: ResolvedConfig }> {
  const resolvedPath = configPath ? path.resolve(expandTildePath(configPath)) : defaultConfigPath();

  if (fs.existsSync(resolvedPath)) {
    const tomlStr = fs.readFileSync(resolvedPath, 'utf-8');
    const parsed: Config = parseConfig(tomlStr);
    const configDir = path.dirname(resolvedPath);
    const dataDir = resolveDataDir(parsed, configDir);

    const config: ResolvedConfig = {
      data_dir: dataDir,
      reporting_currency: parsed.reporting_currency,
      display: parsed.display,
      refresh: parsed.refresh,
      history: parsed.history,
      tray: parsed.tray,
      spending: parsed.spending,
      portfolio: parsed.portfolio,
      ignore: parsed.ignore,
      ai: parsed.ai,
      git: parsed.git,
    };

    return { configPath: resolvedPath, config };
  }

  // File does not exist -- use defaults.  The intended config directory
  // serves as the data directory (mirrors Rust `load_or_default`).
  const configDir = path.dirname(resolvedPath);

  const config: ResolvedConfig = {
    data_dir: configDir,
    reporting_currency: DEFAULT_CONFIG.reporting_currency,
    display: DEFAULT_CONFIG.display,
    refresh: { ...DEFAULT_REFRESH_CONFIG },
    history: { ...DEFAULT_HISTORY_CONFIG },
    tray: {
      ...DEFAULT_TRAY_CONFIG,
      spending_windows_days: [...DEFAULT_TRAY_CONFIG.spending_windows_days],
    },
    spending: {
      ignore_accounts: [...DEFAULT_SPENDING_CONFIG.ignore_accounts],
      ignore_connections: [...DEFAULT_SPENDING_CONFIG.ignore_connections],
      ignore_tags: [...DEFAULT_SPENDING_CONFIG.ignore_tags],
    },
    portfolio: {
      latent_capital_gains_tax: { ...DEFAULT_PORTFOLIO_CONFIG.latent_capital_gains_tax },
    },
    ignore: {
      transaction_rules: [...DEFAULT_IGNORE_CONFIG.transaction_rules],
    },
    ai: {
      openai: { ...DEFAULT_AI_CONFIG.openai },
    },
    git: { ...DEFAULT_GIT_CONFIG },
  };

  return { configPath: resolvedPath, config };
}

// ---------------------------------------------------------------------------
// CLI output
// ---------------------------------------------------------------------------

/**
 * Build the JSON-serialisable output object for the `config` CLI command.
 *
 * Shape matches Rust: `src/app.rs:204-212`.
 */
export function configOutput(configPath: string, config: ResolvedConfig): object {
  return {
    config_file: configPath,
    data_directory: config.data_dir,
    portfolio: config.portfolio,
    git: {
      auto_commit: config.git.auto_commit,
      auto_push: config.git.auto_push,
      pull_before_edit: config.git.pull_before_edit,
      push_after_sync: config.git.push_after_sync,
      merge_master_before_command: config.git.merge_master_before_command,
    },
  };
}

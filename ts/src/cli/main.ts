#!/usr/bin/env node
import { Command } from 'commander';

// App layer
import { loadConfig, configOutput } from '../app/config.js';
import {
  listConnections,
  listAccounts,
  listBalances,
  listTransactions,
  listPriceSources,
  listAll,
} from '../app/list.js';
import {
  addConnection,
  addAccount,
  removeConnection,
  setAccountConfig,
  setBalance,
  setTransactionAnnotation,
} from '../app/mutations.js';
import { importSchwabTransactions } from '../app/import.js';
import {
  fetchHistoricalPrices,
  portfolioSnapshot,
  portfolioHistory,
  portfolioChangePoints,
} from '../app/portfolio.js';
import { spendingReport } from '../app/spending.js';
import {
  syncConnection,
  syncConnectionWithOptions,
  syncConnectionIfStale,
  syncConnectionIfStaleWithOptions,
  syncAll,
  syncAllWithOptions,
  syncAllIfStale,
  syncAllIfStaleWithOptions,
  syncPrices,
  syncRecompact,
  syncSymlinks,
  authLogin,
} from '../app/sync.js';

// Library
import { JsonFileStorage } from '../storage/json-file.js';
import { JsonlMarketDataStore } from '../market-data/jsonl-store.js';
import { tryAutoCommit } from '../git.js';
import { runPreflight } from '../app/preflight.js';

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

async function run(fn: () => Promise<unknown>): Promise<void> {
  try {
    const result = await fn();
    console.log(JSON.stringify(result, null, 2));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    console.log(JSON.stringify({ success: false, error: message }, null, 2));
    process.exit(1);
  }
}

async function runWithConfig(
  fn: (cfg: Awaited<ReturnType<typeof loadConfig>>) => Promise<unknown>,
): Promise<void> {
  await run(async () => {
    const cfg = await loadConfig(program.opts().config);

    const opts = program.opts() as { gitMergeMaster?: boolean; skipGitMergeMaster?: boolean };
    const mergeEnabled = opts.gitMergeMaster
      ? true
      : opts.skipGitMergeMaster
        ? false
        : cfg.config.git.merge_master_before_command;

    await runPreflight(cfg.config, { merge_origin_master: mergeEnabled });
    return fn(cfg);
  });
}

// ---------------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------------

const program = new Command();

program
  .name('keepbook')
  .description('Personal finance tracking CLI')
  .version('0.1.0')
  .option('-c, --config <path>', 'path to config file')
  .option('--git-merge-master', 'merge origin/master before executing the command')
  .option('--skip-git-merge-master', 'skip merging origin/master even if enabled in config');

// ---------------------------------------------------------------------------
// config
// ---------------------------------------------------------------------------

program
  .command('config')
  .description('Print configuration as JSON')
  .action(async () => {
    await runWithConfig(async (cfg) => {
      return configOutput(cfg.configPath, cfg.config);
    });
  });

// ---------------------------------------------------------------------------
// spending
// ---------------------------------------------------------------------------

program
  .command('spending')
  .description('Spending report based on transaction logs')
  .option(
    '--period <period>',
    'period: daily, weekly, monthly, quarterly, yearly, range, custom',
    'monthly',
  )
  .option('--start <date>', 'start date (YYYY-MM-DD)')
  .option('--end <date>', 'end date (YYYY-MM-DD)')
  .option('--currency <code>', 'reporting currency (default: from config)')
  .option('--tz <iana>', 'timezone for bucketing/filtering (IANA name, default: local)')
  .option('--week-start <day>', 'week start: sunday or monday (default: sunday)')
  .option('--bucket <dur>', 'bucket size for period=custom (e.g. 14d)')
  .option('--account <id_or_name>', 'filter to a single account')
  .option('--connection <id_or_name>', 'filter to a single connection')
  .option('--status <status>', 'status: posted, posted+pending, all', 'posted')
  .option('--direction <dir>', 'direction: outflow, inflow, net', 'outflow')
  .option('--group-by <mode>', 'grouping: none, category, merchant, account, tag', 'none')
  .option('--top <n>', 'limit breakdown rows per period', (v: string) => Number.parseInt(v, 10))
  .option(
    '--lookback-days <n>',
    'lookback days for cached close prices / FX (default: 7)',
    (v: string) => Number.parseInt(v, 10),
  )
  .option(
    '--include-noncurrency',
    'include equity/crypto by valuing with cached close prices (default: currency-only)',
    false,
  )
  .option('--include-empty', 'emit empty periods with total 0 (default: sparse output)', false)
  .action(
    async (opts: {
      period: string;
      start?: string;
      end?: string;
      currency?: string;
      tz?: string;
      weekStart?: string;
      bucket?: string;
      account?: string;
      connection?: string;
      status?: string;
      direction?: string;
      groupBy?: string;
      top?: number;
      lookbackDays?: number;
      includeNoncurrency?: boolean;
      includeEmpty?: boolean;
    }) => {
      await runWithConfig(async (cfg) => {
        const storage = new JsonFileStorage(cfg.config.data_dir);
        const marketDataStore = new JsonlMarketDataStore(cfg.config.data_dir);
        return spendingReport(storage, marketDataStore, cfg.config, {
          period: opts.period,
          start: opts.start,
          end: opts.end,
          currency: opts.currency,
          tz: opts.tz,
          week_start: opts.weekStart,
          bucket: opts.bucket,
          account: opts.account,
          connection: opts.connection,
          status: opts.status,
          direction: opts.direction,
          group_by: opts.groupBy,
          top: opts.top,
          lookback_days: opts.lookbackDays,
          include_noncurrency: opts.includeNoncurrency,
          include_empty: opts.includeEmpty,
        });
      });
    },
  );

program
  .command('spending-categories')
  .description('Spending report grouped by category')
  .option(
    '--period <period>',
    'period: daily, weekly, monthly, quarterly, yearly, range, custom',
    'monthly',
  )
  .option('--start <date>', 'start date (YYYY-MM-DD)')
  .option('--end <date>', 'end date (YYYY-MM-DD)')
  .option('--currency <code>', 'reporting currency (default: from config)')
  .option('--tz <iana>', 'timezone for bucketing/filtering (IANA name, default: local)')
  .option('--week-start <day>', 'week start: sunday or monday (default: sunday)')
  .option('--bucket <dur>', 'bucket size for period=custom (e.g. 14d)')
  .option('--account <id_or_name>', 'filter to a single account')
  .option('--connection <id_or_name>', 'filter to a single connection')
  .option('--status <status>', 'status: posted, posted+pending, all', 'posted')
  .option('--direction <dir>', 'direction: outflow, inflow, net', 'outflow')
  .option('--top <n>', 'limit breakdown rows per period', (v: string) => Number.parseInt(v, 10))
  .option(
    '--lookback-days <n>',
    'lookback days for cached close prices / FX (default: 7)',
    (v: string) => Number.parseInt(v, 10),
  )
  .option(
    '--include-noncurrency',
    'include equity/crypto by valuing with cached close prices (default: currency-only)',
    false,
  )
  .option('--include-empty', 'emit empty periods with total 0 (default: sparse output)', false)
  .action(
    async (opts: {
      period: string;
      start?: string;
      end?: string;
      currency?: string;
      tz?: string;
      weekStart?: string;
      bucket?: string;
      account?: string;
      connection?: string;
      status?: string;
      direction?: string;
      top?: number;
      lookbackDays?: number;
      includeNoncurrency?: boolean;
      includeEmpty?: boolean;
    }) => {
      await runWithConfig(async (cfg) => {
        const storage = new JsonFileStorage(cfg.config.data_dir);
        const marketDataStore = new JsonlMarketDataStore(cfg.config.data_dir);
        return spendingReport(storage, marketDataStore, cfg.config, {
          period: opts.period,
          start: opts.start,
          end: opts.end,
          currency: opts.currency,
          tz: opts.tz,
          week_start: opts.weekStart,
          bucket: opts.bucket,
          account: opts.account,
          connection: opts.connection,
          status: opts.status,
          direction: opts.direction,
          group_by: 'category',
          top: opts.top,
          lookback_days: opts.lookbackDays,
          include_noncurrency: opts.includeNoncurrency,
          include_empty: opts.includeEmpty,
        });
      });
    },
  );

// ---------------------------------------------------------------------------
// add
// ---------------------------------------------------------------------------

const add = program.command('add').description('Add a resource');

add
  .command('connection <name>')
  .description('Add a new connection')
  .option('--synchronizer <name>', 'synchronizer to use (default: manual)', 'manual')
  .action(async (name: string, opts: { synchronizer: string }) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      const result = await addConnection(storage, name, opts.synchronizer);
      if (cfg.config.git.auto_commit) {
        await tryAutoCommit(
          cfg.config.data_dir,
          `add connection '${name}'`,
          cfg.config.git.auto_push,
        );
      }
      return result;
    });
  });

add
  .command('account <name>')
  .description('Add a new account')
  .requiredOption('--connection <id>', 'connection ID')
  .option(
    '--tag <tag>',
    'tag (repeatable)',
    (val: string, arr: string[]) => [...arr, val],
    [] as string[],
  )
  .action(async (name: string, opts: { connection: string; tag: string[] }) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      const result = await addAccount(storage, opts.connection, name, opts.tag);
      if (cfg.config.git.auto_commit) {
        await tryAutoCommit(cfg.config.data_dir, `add account '${name}'`, cfg.config.git.auto_push);
      }
      return result;
    });
  });

// ---------------------------------------------------------------------------
// remove
// ---------------------------------------------------------------------------

const remove = program.command('remove').description('Remove a resource');

remove
  .command('connection <id>')
  .description('Remove a connection and its accounts')
  .action(async (id: string) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      const result = await removeConnection(storage, id);
      if (cfg.config.git.auto_commit) {
        await tryAutoCommit(
          cfg.config.data_dir,
          `remove connection '${id}'`,
          cfg.config.git.auto_push,
        );
      }
      return result;
    });
  });

// ---------------------------------------------------------------------------
// set
// ---------------------------------------------------------------------------

const set = program.command('set').description('Set a value');

set
  .command('balance')
  .description('Set a balance for an account')
  .requiredOption('--account <id>', 'account ID')
  .requiredOption('--asset <str>', 'asset identifier')
  .requiredOption('--amount <str>', 'balance amount')
  .action(async (opts: { account: string; asset: string; amount: string }) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      const result = await setBalance(storage, opts.account, opts.asset, opts.amount);
      if (cfg.config.git.auto_commit) {
        await tryAutoCommit(
          cfg.config.data_dir,
          `set balance for '${opts.account}'`,
          cfg.config.git.auto_push,
        );
      }
      return result;
    });
  });

set
  .command('account-config')
  .description('Set account-level configuration values')
  .requiredOption('--account <id_or_name>', 'account ID or name')
  .option('--balance-backfill <policy>', 'balance backfill policy: none, zero, carry_earliest')
  .option('--clear-balance-backfill', 'clear balance backfill policy override')
  .action(
    async (opts: { account: string; balanceBackfill?: string; clearBalanceBackfill?: boolean }) => {
      await runWithConfig(async (cfg) => {
        const storage = new JsonFileStorage(cfg.config.data_dir);
        const result = await setAccountConfig(storage, opts.account, {
          balance_backfill: opts.balanceBackfill,
          clear_balance_backfill: opts.clearBalanceBackfill,
        });
        if ((result as { success?: boolean }).success && cfg.config.git.auto_commit) {
          await tryAutoCommit(
            cfg.config.data_dir,
            `set account config '${opts.account}'`,
            cfg.config.git.auto_push,
          );
        }
        return result;
      });
    },
  );

set
  .command('transaction')
  .description('Set a transaction annotation (append-only patch)')
  .requiredOption('--account <id>', 'account ID')
  .requiredOption('--transaction <id>', 'transaction ID')
  .option('--description <text>', 'override description')
  .option('--clear-description', 'clear description override')
  .option('--note <text>', 'set note')
  .option('--clear-note', 'clear note')
  .option('--category <text>', 'set category')
  .option('--clear-category', 'clear category')
  .option(
    '--tag <tag>',
    'tag (repeatable)',
    (val: string, arr: string[]) => [...arr, val],
    [] as string[],
  )
  .option('--tags-empty', 'set tags to empty array')
  .option('--clear-tags', 'clear tags field')
  .action(
    async (opts: {
      account: string;
      transaction: string;
      description?: string;
      clearDescription?: boolean;
      note?: string;
      clearNote?: boolean;
      category?: string;
      clearCategory?: boolean;
      tag: string[];
      tagsEmpty?: boolean;
      clearTags?: boolean;
    }) => {
      await runWithConfig(async (cfg) => {
        const storage = new JsonFileStorage(cfg.config.data_dir);
        const result = await setTransactionAnnotation(storage, opts.account, opts.transaction, {
          description: opts.description,
          clear_description: opts.clearDescription,
          note: opts.note,
          clear_note: opts.clearNote,
          category: opts.category,
          clear_category: opts.clearCategory,
          tags: opts.tag,
          tags_empty: opts.tagsEmpty,
          clear_tags: opts.clearTags,
        });
        if ((result as { success?: boolean }).success && cfg.config.git.auto_commit) {
          await tryAutoCommit(
            cfg.config.data_dir,
            `set transaction annotation '${opts.transaction}'`,
            cfg.config.git.auto_push,
          );
        }
        return result;
      });
    },
  );

// ---------------------------------------------------------------------------
// import
// ---------------------------------------------------------------------------

const importCmd = program.command('import').description('Import data from exported files');

const importSchwab = importCmd.command('schwab').description('Schwab import commands');

importSchwab
  .command('transactions <file>')
  .description('Import transactions from a Schwab JSON export file')
  .requiredOption('--account <id_or_name>', 'account ID or name')
  .action(async (file: string, opts: { account: string }) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      const result = await importSchwabTransactions(storage, opts.account, file);
      if (cfg.config.git.auto_commit) {
        await tryAutoCommit(
          cfg.config.data_dir,
          `import schwab transactions (account ${result.account_id})`,
          cfg.config.git.auto_push,
        );
      }
      return result;
    });
  });

// ---------------------------------------------------------------------------
// list
// ---------------------------------------------------------------------------

const list = program.command('list').description('List resources');

list
  .command('connections')
  .description('List all connections')
  .action(async () => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      return listConnections(storage);
    });
  });

list
  .command('accounts')
  .description('List all accounts')
  .action(async () => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      return listAccounts(storage);
    });
  });

list
  .command('balances')
  .description('List latest balances')
  .action(async () => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      const marketDataStore = new JsonlMarketDataStore(cfg.config.data_dir);
      return listBalances(storage, cfg.config, marketDataStore);
    });
  });

list
  .command('transactions')
  .description('List all transactions')
  .option('--start <date>', 'start date (YYYY-MM-DD, default: 30 days ago)')
  .option('--end <date>', 'end date (YYYY-MM-DD, default: today)')
  .option('--sort-by-amount', 'sort transactions by amount (ascending)')
  .option('--include-ignored', 'include transactions ignored by spending/list ignore rules')
  .action(
    async (opts: {
      start?: string;
      end?: string;
      sortByAmount?: boolean;
      includeIgnored?: boolean;
    }) => {
      await runWithConfig(async (cfg) => {
        const storage = new JsonFileStorage(cfg.config.data_dir);
        return listTransactions(
          storage,
          opts.start,
          opts.end,
          cfg.config,
          opts.sortByAmount === true,
          opts.includeIgnored !== true,
        );
      });
    },
  );

list
  .command('price-sources')
  .description('List price sources')
  .action(async () => {
    await runWithConfig(async (cfg) => {
      return listPriceSources(cfg.config.data_dir);
    });
  });

list
  .command('all')
  .description('List everything')
  .action(async () => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      const marketDataStore = new JsonlMarketDataStore(cfg.config.data_dir);
      return listAll(storage, cfg.config, marketDataStore);
    });
  });

// ---------------------------------------------------------------------------
// tui
// ---------------------------------------------------------------------------

program
  .command('tui')
  .description('Interactive terminal interface')
  .option('--view <view>', 'initial view: transactions|net-worth', 'transactions')
  .option(
    '--net-worth-interval <interval>',
    'net-worth update interval: full|hourly|daily|weekly|monthly|yearly',
    'daily',
  )
  .action(async (_opts: { view?: string; netWorthInterval?: string }) => {
    await run(async () => {
      return {
        success: false,
        error:
          'The TUI is currently implemented in the Rust CLI only. Use the Rust keepbook binary for `tui`.',
      };
    });
  });

// ---------------------------------------------------------------------------
// sync
// ---------------------------------------------------------------------------

const sync = program.command('sync').description('Synchronize data');

sync
  .command('connection <id_or_name>')
  .description('Sync a single connection')
  .option('--if-stale', 'only sync if data is stale')
  .option('--transactions <mode>', 'transaction sync mode: auto|full', 'auto')
  .action(async (idOrName: string, opts: { ifStale?: boolean; transactions?: string }) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      const txMode = opts.transactions === 'full' ? 'full' : 'auto';
      if (opts.ifStale) {
        return syncConnectionIfStaleWithOptions(storage, idOrName, cfg.config.refresh, {
          transactions: txMode,
        });
      }
      return syncConnectionWithOptions(storage, idOrName, { transactions: txMode });
    });
  });

sync
  .command('all')
  .description('Sync all connections')
  .option('--if-stale', 'only sync if data is stale')
  .option('--transactions <mode>', 'transaction sync mode: auto|full', 'auto')
  .action(async (opts: { ifStale?: boolean; transactions?: string }) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      const txMode = opts.transactions === 'full' ? 'full' : 'auto';
      if (opts.ifStale) {
        return syncAllIfStaleWithOptions(storage, cfg.config.refresh, { transactions: txMode });
      }
      return syncAllWithOptions(storage, { transactions: txMode });
    });
  });

sync
  .command('prices [scope] [id]')
  .description('Sync prices')
  .option('--force', 'force refresh')
  .option('--quote-staleness <dur>', 'quote staleness duration')
  .action(async (_scope?: string, _id?: string, _opts?: object) => {
    await runWithConfig(async (_cfg) => {
      return syncPrices();
    });
  });

sync
  .command('symlinks')
  .description('Create symlinks')
  .action(async () => {
    await runWithConfig(async (_cfg) => {
      return syncSymlinks();
    });
  });

sync
  .command('recompact')
  .description('Recompact account JSONL files (dedupe append-only logs and sort chronologically)')
  .action(async () => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      return syncRecompact(storage);
    });
  });

// ---------------------------------------------------------------------------
// auth
// ---------------------------------------------------------------------------

const auth = program.command('auth').description('Authentication commands');

const authSchwab = auth.command('schwab').description('Schwab authentication');
authSchwab
  .command('login [id_or_name]')
  .description('Login to Schwab')
  .action(async (idOrName?: string) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      return authLogin(storage, 'schwab', idOrName);
    });
  });

const authChase = auth.command('chase').description('Chase authentication');
authChase
  .command('login [id_or_name]')
  .description('Login to Chase')
  .action(async (idOrName?: string) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      return authLogin(storage, 'chase', idOrName);
    });
  });

// ---------------------------------------------------------------------------
// market-data
// ---------------------------------------------------------------------------

const marketData = program.command('market-data').description('Market data commands');

marketData
  .command('fetch')
  .description('Fetch historical prices for assets in scope')
  .option('--account <id_or_name>', 'account ID or name (mutually exclusive with --connection)')
  .option('--connection <id_or_name>', 'connection ID or name (mutually exclusive with --account)')
  .option('--start <date>', 'start date (YYYY-MM-DD, default: earliest balance date in scope)')
  .option('--end <date>', 'end date (YYYY-MM-DD, default: today)')
  .option('--interval <interval>', 'interval: daily, weekly, monthly, yearly/annual', 'monthly')
  .option(
    '--lookback-days <days>',
    'look back this many days when a close price is missing',
    (v: string) => Number.parseInt(v, 10),
  )
  .option('--request-delay-ms <ms>', 'delay (ms) between price fetches', (v: string) =>
    Number.parseInt(v, 10),
  )
  .option('--currency <code>', 'base currency for FX rates (default: from config)')
  .option('--no-fx', 'disable FX rate fetching')
  .action(
    async (opts: {
      account?: string;
      connection?: string;
      start?: string;
      end?: string;
      interval?: string;
      lookbackDays?: number;
      requestDelayMs?: number;
      currency?: string;
      fx?: boolean;
    }) => {
      await runWithConfig(async (cfg) => {
        const storage = new JsonFileStorage(cfg.config.data_dir);
        return fetchHistoricalPrices(storage, cfg.config, {
          account: opts.account,
          connection: opts.connection,
          start: opts.start,
          end: opts.end,
          interval: opts.interval,
          lookback_days: opts.lookbackDays,
          request_delay_ms: opts.requestDelayMs,
          currency: opts.currency,
          include_fx: opts.fx,
        });
      });
    },
  );

// ---------------------------------------------------------------------------
// portfolio
// ---------------------------------------------------------------------------

const portfolio = program.command('portfolio').description('Portfolio commands');

portfolio
  .command('snapshot')
  .description('Portfolio snapshot')
  .option('--currency <code>', 'reporting currency')
  .option('--date <date>', 'as-of date (YYYY-MM-DD)')
  .option('--group-by <grouping>', 'grouping: asset, account, or both')
  .option('--detail', 'include holding detail')
  .option('--auto', 'auto mode')
  .option('--offline', 'offline mode')
  .option('--dry-run', 'dry run')
  .option('--force-refresh', 'force refresh prices')
  .action(
    async (opts: { currency?: string; date?: string; groupBy?: string; detail?: boolean }) => {
      await runWithConfig(async (cfg) => {
        const storage = new JsonFileStorage(cfg.config.data_dir);
        const marketDataStore = new JsonlMarketDataStore(cfg.config.data_dir);
        return portfolioSnapshot(storage, marketDataStore, cfg.config, {
          currency: opts.currency,
          date: opts.date,
          groupBy: opts.groupBy,
          detail: opts.detail,
        });
      });
    },
  );

portfolio
  .command('history')
  .description('Portfolio history')
  .option('--currency <code>', 'reporting currency')
  .option('--start <date>', 'start date (YYYY-MM-DD)')
  .option('--end <date>', 'end date (YYYY-MM-DD)')
  .option('--granularity <granularity>', 'granularity: none, daily, weekly, monthly')
  .option('--include-prices', 'include price change points', true)
  .option('--no-include-prices', 'exclude price change points')
  .action(
    async (opts: {
      currency?: string;
      start?: string;
      end?: string;
      granularity?: string;
      includePrices?: boolean;
    }) => {
      await runWithConfig(async (cfg) => {
        const storage = new JsonFileStorage(cfg.config.data_dir);
        const marketDataStore = new JsonlMarketDataStore(cfg.config.data_dir);
        return portfolioHistory(storage, marketDataStore, cfg.config, {
          currency: opts.currency,
          start: opts.start,
          end: opts.end,
          granularity: opts.granularity,
          includePrices: opts.includePrices,
        });
      });
    },
  );

portfolio
  .command('change-points')
  .description('Portfolio change points')
  .option('--start <date>', 'start date (YYYY-MM-DD)')
  .option('--end <date>', 'end date (YYYY-MM-DD)')
  .option('--granularity <granularity>', 'granularity: none, daily, weekly, monthly')
  .option('--include-prices', 'include price change points', true)
  .option('--no-include-prices', 'exclude price change points')
  .action(
    async (opts: {
      start?: string;
      end?: string;
      granularity?: string;
      includePrices?: boolean;
    }) => {
      await runWithConfig(async (cfg) => {
        const storage = new JsonFileStorage(cfg.config.data_dir);
        const marketDataStore = new JsonlMarketDataStore(cfg.config.data_dir);
        return portfolioChangePoints(storage, marketDataStore, cfg.config, {
          start: opts.start,
          end: opts.end,
          granularity: opts.granularity,
          includePrices: opts.includePrices,
        });
      });
    },
  );

// ---------------------------------------------------------------------------
// Parse and execute
// ---------------------------------------------------------------------------

program.parseAsync(process.argv);

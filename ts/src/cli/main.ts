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
import { addConnection, addAccount, removeConnection, setBalance } from '../app/mutations.js';
import { portfolioSnapshot, portfolioHistory, portfolioChangePoints } from '../app/portfolio.js';
import { syncConnection, syncAll, syncPrices, syncSymlinks, authLogin } from '../app/sync.js';

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
  .option(
    '--skip-git-merge-master',
    'skip merging origin/master even if enabled in config',
  );

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
// add
// ---------------------------------------------------------------------------

const add = program.command('add').description('Add a resource');

add
  .command('connection <name>')
  .description('Add a new manual connection')
  .action(async (name: string) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      const result = await addConnection(storage, name);
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
      return listBalances(storage, cfg.config.reporting_currency, marketDataStore);
    });
  });

list
  .command('transactions')
  .description('List all transactions')
  .action(async () => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      return listTransactions(storage);
    });
  });

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
      return listAll(storage, cfg.config.reporting_currency, marketDataStore, cfg.config.data_dir);
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
  .action(async (idOrName: string, _opts: { ifStale?: boolean }) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      return syncConnection(storage, idOrName);
    });
  });

sync
  .command('all')
  .description('Sync all connections')
  .option('--if-stale', 'only sync if data is stale')
  .action(async (_opts: { ifStale?: boolean }) => {
    await runWithConfig(async (cfg) => {
      const storage = new JsonFileStorage(cfg.config.data_dir);
      return syncAll(storage);
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

// ---------------------------------------------------------------------------
// auth
// ---------------------------------------------------------------------------

const auth = program.command('auth').description('Authentication commands');

const authSchwab = auth.command('schwab').description('Schwab authentication');
authSchwab
  .command('login [id_or_name]')
  .description('Login to Schwab')
  .action(async (idOrName?: string) => {
    await runWithConfig(async (_cfg) => {
      return authLogin('schwab', idOrName);
    });
  });

const authChase = auth.command('chase').description('Chase authentication');
authChase
  .command('login [id_or_name]')
  .description('Login to Chase')
  .action(async (idOrName?: string) => {
    await runWithConfig(async (_cfg) => {
      return authLogin('chase', idOrName);
    });
  });

// ---------------------------------------------------------------------------
// market-data
// ---------------------------------------------------------------------------

const marketData = program.command('market-data').description('Market data commands');

marketData
  .command('fetch')
  .description('Fetch market data')
  .option('--account <id>', 'account ID or name')
  .option('--connection <id>', 'connection ID or name')
  .option('--start <date>', 'start date')
  .option('--end <date>', 'end date')
  .option('--interval <interval>', 'interval')
  .option('--lookback-days <days>', 'lookback days')
  .option('--request-delay-ms <ms>', 'request delay in ms')
  .option('--currency <code>', 'currency code')
  .option('--no-fx', 'disable FX conversion')
  .action(async () => {
    await runWithConfig(async (_cfg) => {
      return { success: false, error: 'Market data fetch not yet implemented in TypeScript CLI' };
    });
  });

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

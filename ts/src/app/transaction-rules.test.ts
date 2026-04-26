import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

import { describe, it, expect } from 'vitest';

import type { ResolvedConfig } from '../config.js';
import { FixedClock } from '../clock.js';
import { Asset } from '../models/asset.js';
import { Account } from '../models/account.js';
import { Id } from '../models/id.js';
import { FixedIdGenerator } from '../models/id-generator.js';
import { Transaction } from '../models/transaction.js';
import { MemoryStorage } from '../storage/memory.js';
import {
  appendTransactionAnnotationRule,
  applyTransactionAnnotationRules,
  exactCaseInsensitivePattern,
} from './transaction-rules.js';

function makeConfig(overrides?: Partial<ResolvedConfig>): ResolvedConfig {
  return {
    data_dir: '/tmp/test',
    reporting_currency: 'USD',
    display: {},
    refresh: {
      balance_staleness: 14 * 86400000,
      price_staleness: 86400000,
    },
    history: { allow_future_projection: false },
    tray: { history_points: 8, spending_windows_days: [7, 30, 90] },
    spending: { ignore_accounts: [], ignore_connections: [], ignore_tags: [] },
    portfolio: {
      latent_capital_gains_tax: { enabled: false, account_name: 'Latent Capital Gains Tax' },
    },
    ignore: { transaction_rules: [] },
    git: { auto_commit: false, auto_push: false, merge_master_before_command: false },
    ...overrides,
  };
}

describe('applyTransactionAnnotationRules', () => {
  it('fills missing category and description override from matching rules', async () => {
    const storage = new MemoryStorage();
    const accountId = Id.fromString('acct-1');
    const connectionId = Id.fromString('conn-1');
    await storage.saveAccount(
      Account.newWith(accountId, new Date('2024-06-01T00:00:00Z'), 'Checking', connectionId),
    );

    const clock = new FixedClock(new Date('2024-06-15T12:00:00Z'));
    const tx = Transaction.newWithGenerator(
      new FixedIdGenerator([Id.fromString('tx-1')]),
      clock,
      '-4448.88',
      Asset.currency('USD'),
      'ACH DEBIT RENT PORTAL',
    );
    await storage.appendTransactions(accountId, [tx]);

    const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), 'keepbook-rules-'));
    const rulesPath = path.join(tmpDir, 'transaction_category_rules.jsonl');
    await appendTransactionAnnotationRule(rulesPath, {
      category: 'Rent',
      description_override: 'Rent - 100 Broderick',
      account_name: exactCaseInsensitivePattern('Checking'),
      description: '(?i)rent portal',
      amount: '^-4448\\.88$',
    });

    const result = await applyTransactionAnnotationRules(
      storage,
      makeConfig({ data_dir: tmpDir }),
      { rules_path: rulesPath },
      clock,
    );

    expect(result).toMatchObject({
      success: true,
      rules_loaded: 1,
      transactions_matched: 1,
      annotations_written: 1,
      changes: [
        {
          rule_index: 0,
          account_id: 'acct-1',
          account_name: 'Checking',
          transaction_id: 'tx-1',
          category: 'Rent',
          description: 'Rent - 100 Broderick',
        },
      ],
    });

    const second = await applyTransactionAnnotationRules(
      storage,
      makeConfig({ data_dir: tmpDir }),
      { rules_path: rulesPath },
      clock,
    );
    expect(second).toMatchObject({
      transactions_matched: 1,
      annotations_written: 0,
      changes: [],
    });
  });
});

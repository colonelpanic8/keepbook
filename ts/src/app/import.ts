import fs from 'node:fs/promises';

import type { Storage } from '../storage/storage.js';
import { findAccount } from '../storage/lookup.js';
import { parseSchwabExportedTransactionsJson } from '../sync/schwab.js';

export type ImportSchwabTransactionsOutput = {
  success: boolean;
  account_id: string;
  imported: number;
  skipped: number;
};

export async function importSchwabTransactions(
  storage: Storage,
  accountIdOrName: string,
  file: string,
): Promise<ImportSchwabTransactionsOutput> {
  const account = await findAccount(storage, accountIdOrName);
  if (account === null) {
    throw new Error(`Account not found: ${accountIdOrName}`);
  }

  const contents = await fs.readFile(file, 'utf8');
  const parsed = parseSchwabExportedTransactionsJson(account.id, contents);
  if (parsed.transactions.length > 0) {
    await storage.appendTransactions(account.id, parsed.transactions);
  }

  return {
    success: true,
    account_id: account.id.asStr(),
    imported: parsed.transactions.length,
    skipped: parsed.skipped,
  };
}


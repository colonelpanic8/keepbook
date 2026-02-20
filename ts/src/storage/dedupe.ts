import { type TransactionType } from '../models/transaction.js';

function asObject(value: unknown): Record<string, unknown> | null {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function nonEmptyString(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function transactionDedupeKeys(tx: TransactionType): string[] {
  const keys = new Set<string>([`id:${tx.id.asStr()}`]);

  // Chase can represent the same transaction with different stable id sources
  // across API versions/sync paths. Treat these ids as aliases.
  const syncData = asObject(tx.synchronizer_data);
  if (syncData !== null && 'chase_account_id' in syncData) {
    const aliasFields = [
      'stable_id',
      'sor_transaction_identifier',
      'derived_unique_transaction_identifier',
      'transaction_reference_number',
    ] as const;
    for (const field of aliasFields) {
      const value = nonEmptyString(syncData[field]);
      if (value !== null) {
        keys.add(`chase:${field}:${value}`);
        keys.add(`chase:alias:${value}`);
      }
    }
  }

  return Array.from(keys.values()).sort();
}

export function dedupeTransactionsLastWriteWins(txns: TransactionType[]): TransactionType[] {
  const keyToIndex = new Map<string, number>();
  const indexToKeys = new Map<number, Set<string>>();
  const deduped: Array<TransactionType | null> = [];

  for (const tx of txns) {
    const keys = transactionDedupeKeys(tx);
    const matched = new Set<number>();
    for (const key of keys) {
      const idx = keyToIndex.get(key);
      if (idx !== undefined && deduped[idx] !== null) {
        matched.add(idx);
      }
    }

    let targetIndex: number;
    if (matched.size === 0) {
      targetIndex = deduped.length;
      deduped.push(tx);
    } else {
      targetIndex = Math.min(...matched);
      deduped[targetIndex] = tx;
    }

    for (const idx of matched) {
      if (idx === targetIndex) continue;
      deduped[idx] = null;
      const idxKeys = indexToKeys.get(idx);
      if (!idxKeys) continue;
      const targetKeys = indexToKeys.get(targetIndex) ?? new Set<string>();
      for (const key of idxKeys) {
        keyToIndex.set(key, targetIndex);
        targetKeys.add(key);
      }
      indexToKeys.set(targetIndex, targetKeys);
      indexToKeys.delete(idx);
    }

    const targetKeys = indexToKeys.get(targetIndex) ?? new Set<string>();
    for (const key of keys) {
      keyToIndex.set(key, targetIndex);
      targetKeys.add(key);
    }
    indexToKeys.set(targetIndex, targetKeys);
  }

  return deduped.filter((tx): tx is TransactionType => tx !== null);
}

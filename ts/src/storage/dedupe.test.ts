import { describe, it, expect } from 'vitest';

import { Asset } from '../models/asset.js';
import { FixedIdGenerator } from '../models/id-generator.js';
import { Id } from '../models/id.js';
import { Transaction, withId, withSynchronizerData } from '../models/transaction.js';
import { FixedClock } from '../clock.js';
import { dedupeTransactionsLastWriteWins } from './dedupe.js';

function makeChaseTx(
  id: string,
  stableId: string,
  opts: { sorId?: string; derivedId?: string } = {},
) {
  const base = Transaction.newWithGenerator(
    new FixedIdGenerator([Id.fromString(`seed-${id}`)]),
    new FixedClock(new Date('2026-02-20T12:00:00.000Z')),
    '-10',
    Asset.currency('USD'),
    'Test',
  );
  const syncData: Record<string, unknown> = {
    chase_account_id: 123,
    stable_id: stableId,
  };
  if (opts.sorId !== undefined) {
    syncData.sor_transaction_identifier = opts.sorId;
  }
  if (opts.derivedId !== undefined) {
    syncData.derived_unique_transaction_identifier = opts.derivedId;
  }
  return withSynchronizerData(withId(base, Id.fromString(id)), syncData);
}

describe('dedupeTransactionsLastWriteWins', () => {
  it('collapses chase alias ids to a single transaction', () => {
    const old = makeChaseTx('tx-old', '202602151536556260124#20260124');
    const newer = makeChaseTx('tx-new', '466046216565116');
    const newest = makeChaseTx('tx-new', '466046216565116', {
      sorId: '466046216565116',
      derivedId: '202602151536556260124#20260124',
    });

    const out = dedupeTransactionsLastWriteWins([old, newer, newest]);
    expect(out).toHaveLength(1);
    expect(out[0].id.asStr()).toBe('tx-new');
  });
});
